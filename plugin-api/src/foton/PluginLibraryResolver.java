package foton;

import java.io.FileNotFoundException;
import java.io.IOException;
import java.io.InputStream;
import java.net.HttpURLConnection;
import java.net.URI;
import java.net.URL;
import java.nio.file.Files;
import java.nio.file.LinkOption;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HexFormat;
import java.util.List;
import java.util.concurrent.atomic.AtomicInteger;
import org.apache.maven.repository.internal.MavenRepositorySystemUtils;
import org.eclipse.aether.RepositorySystem;
import org.eclipse.aether.RepositorySystemSession;
import org.eclipse.aether.artifact.Artifact;
import org.eclipse.aether.collection.CollectRequest;
import org.eclipse.aether.connector.basic.BasicRepositoryConnectorFactory;
import org.eclipse.aether.graph.Dependency;
import org.eclipse.aether.repository.LocalRepository;
import org.eclipse.aether.repository.RemoteRepository;
import org.eclipse.aether.repository.RepositoryPolicy;
import org.eclipse.aether.resolution.DependencyRequest;
import org.eclipse.aether.spi.connector.RepositoryConnectorFactory;
import org.eclipse.aether.spi.connector.transport.AbstractTransporter;
import org.eclipse.aether.spi.connector.transport.GetTask;
import org.eclipse.aether.spi.connector.transport.PeekTask;
import org.eclipse.aether.spi.connector.transport.PutTask;
import org.eclipse.aether.spi.connector.transport.Transporter;
import org.eclipse.aether.spi.connector.transport.TransporterFactory;
import org.eclipse.aether.transfer.NoTransporterException;

/** Real Maven graph resolution with bounded transfers and content-addressed JAR publication.
 * POM-declared repositories are deliberately ignored: only the plugin's explicit repositories
 * may receive requests. The Maven graph/model builder is the same version used by Paper.
 */
public final class PluginLibraryResolver {
    private static final ThreadLocal<Path> PLUGIN_CACHE = new ThreadLocal<>();
    private static final long MAX_BYTES = 64L * 1024 * 1024;
    private static final long MAX_EXPANDED_BYTES = 256L * 1024 * 1024;
    private static final long MAX_TRANSFER_BYTES = 256L * 1024 * 1024;
    private static final long MAX_CACHE_BYTES = 1024L * 1024 * 1024;
    private static final int MAX_REQUESTS = 512;
    private static final int MAX_ARTIFACTS = 128;
    private static final long TIME_LIMIT_NANOS = 120_000_000_000L;

    private PluginLibraryResolver() {}

    public static Path cacheDirectory() {
        Path cache = PLUGIN_CACHE.get();
        return cache == null ? Path.of("libraries") : cache;
    }

    static List<URL> registerLibraries(Path cache,
            List<io.papermc.paper.plugin.loader.library.ClassPathLibrary> libraries) {
        Path previous = PLUGIN_CACHE.get();
        PLUGIN_CACHE.set(cache);
        try {
            List<URL> urls = new ArrayList<>();
            for (var library : libraries) library.register(path -> {
                try { urls.add(localLibrary(path)); }
                catch (IOException error) { throw new io.papermc.paper.plugin.loader.library.LibraryLoadingException("Invalid plugin library " + path, error); }
            });
            return List.copyOf(urls);
        } finally {
            if (previous == null) PLUGIN_CACHE.remove();
            else PLUGIN_CACHE.set(previous);
        }
    }

    public static List<URL> resolve(Path cache, List<Dependency> dependencies,
            List<RemoteRepository> repositories) throws Exception {
        if (dependencies.size() > MAX_ARTIFACTS || repositories.size() > 16) {
            throw new IOException("Plugin library graph exceeds dependency/repository limit");
        }
        for (Dependency dependency : dependencies) validateArtifact(dependency.getArtifact());
        for (RemoteRepository repository : repositories) validateRepository(URI.create(repository.getUrl()));
        if (dependencies.isEmpty()) return List.of();
        if (repositories.isEmpty()) throw new IOException("No repositories configured for plugin libraries");
        Path root = directory(cache);
        try (var paths = Files.walk(root)) {
            long size = 0;
            int entries = 0;
            for (Path path : (Iterable<Path>) paths::iterator) {
                if (++entries > 10_000) throw new IOException("Plugin library cache exceeds entry limit");
                if (Files.isSymbolicLink(path)) throw new IOException("Plugin library cache contains a symbolic link: " + path);
                if (Files.isRegularFile(path, LinkOption.NOFOLLOW_LINKS)) size += Files.size(path);
                if (size > MAX_CACHE_BYTES) throw new IOException("Plugin library cache exceeds 1 GiB; move unused cache entries before retrying");
            }
        }
        Path temporary = Files.createTempDirectory(root, ".resolution-");
        BoundedTransportFactory transport = null;
        try {
            var locator = MavenRepositorySystemUtils.newServiceLocator();
            locator.addService(RepositoryConnectorFactory.class, BasicRepositoryConnectorFactory.class);
            transport = new BoundedTransportFactory(root);
            locator.setServices(TransporterFactory.class, transport);
            locator.setErrorHandler(new org.eclipse.aether.impl.DefaultServiceLocator.ErrorHandler() {
                @Override public void serviceCreationFailed(Class<?> type, Class<?> implementation, Throwable error) {
                    throw new IllegalStateException("Cannot initialize Maven resolver " + implementation.getName(), error);
                }
            });
            RepositorySystem system = locator.getService(RepositorySystem.class);
            if (system == null) throw new IOException("Maven resolver runtime is incomplete");
            var session = MavenRepositorySystemUtils.newSession();
            session.setSystemProperties(System.getProperties());
            BoundedTransportFactory responseCache = transport;
            session.setTransferListener(new org.eclipse.aether.transfer.AbstractTransferListener() {
                @Override public void transferCorrupted(org.eclipse.aether.transfer.TransferEvent event)
                        throws org.eclipse.aether.transfer.TransferCancelledException {
                    try { responseCache.invalidate(); }
                    catch (IOException error) {
                        throw new org.eclipse.aether.transfer.TransferCancelledException("Cannot discard rejected Maven responses", error);
                    }
                }
            });
            session.setChecksumPolicy(RepositoryPolicy.CHECKSUM_POLICY_FAIL);
            session.setArtifactDescriptorPolicy(new org.eclipse.aether.util.repository.SimpleArtifactDescriptorPolicy(false, false));
            session.setIgnoreArtifactDescriptorRepositories(true);
            session.setConfigProperty("aether.connector.basic.threads", 1);
            session.setConfigProperty("aether.dependencyCollector.impl", "df");
            session.setLocalRepositoryManager(system.newLocalRepositoryManager(session, new LocalRepository(temporary.toFile())));
            session.setReadOnly();
            var graph = system.collectDependencies(session, new CollectRequest((Dependency) null, dependencies, repositories));
            var pending = new java.util.ArrayDeque<org.eclipse.aether.graph.DependencyNode>();
            var seen = java.util.Collections.newSetFromMap(new java.util.IdentityHashMap<org.eclipse.aether.graph.DependencyNode, Boolean>());
            pending.add(graph.getRoot());
            while (!pending.isEmpty()) {
                var node = pending.removeFirst();
                if (!seen.add(node)) continue;
                if (seen.size() > MAX_ARTIFACTS + 1) throw new IOException("Plugin library graph exceeds artifact limit");
                if (node.getArtifact() != null) {
                    validateArtifact(node.getArtifact());
                    if (node.getArtifact().getProperty("localPath", null) != null
                            || (node.getDependency() != null && "system".equals(node.getDependency().getScope()))) {
                        throw new IOException("Maven system paths are not plugin libraries; register an explicit JarLibrary");
                    }
                }
                pending.addAll(node.getChildren());
            }
            var result = system.resolveDependencies(session, new DependencyRequest(graph.getRoot(), null));
            if (result.getArtifactResults().size() > MAX_ARTIFACTS) throw new IOException("Plugin library graph exceeds artifact limit");
            List<URL> urls = new ArrayList<>();
            for (var artifact : result.getArtifactResults()) {
                validateArtifact(artifact.getArtifact());
                Path source = artifact.getArtifact().getFile().toPath();
                validateJar(source);
                String digest = digest(source);
                Path published = root.resolve(digest + ".jar");
                if (!verified(published, digest)) {
                    Files.move(source, published, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
                }
                urls.add(published.toUri().toURL());
            }
            return List.copyOf(urls);
        } catch (Exception error) {
            // Self-digests detect local corruption, not repository checksum validity.
            // Discard this attempt's responses so a corrected mirror can be retried.
            if (transport != null) {
                try { transport.invalidate(); }
                catch (IOException cleanup) { error.addSuppressed(cleanup); }
            }
            throw error;
        } finally {
            try (var paths = Files.walk(temporary)) {
                for (Path path : paths.sorted(Comparator.reverseOrder()).toList()) Files.deleteIfExists(path);
            }
        }
    }

    public static URL localLibrary(Path path) throws IOException {
        for (Path component : path) {
            if (component.toString().equals("..")) throw new IOException("Library path contains traversal: " + path);
        }
        Path real = path.toRealPath();
        validateJar(real);
        return real.toUri().toURL();
    }

    private static void validateArtifact(Artifact artifact) throws IOException {
        for (String value : List.of(artifact.getGroupId(), artifact.getArtifactId(), artifact.getExtension())) {
            if (!value.matches("[A-Za-z0-9_][A-Za-z0-9_.+-]{0,199}") || value.contains("..")) {
                throw new IOException("Invalid plugin Maven coordinate: " + artifact);
            }
        }
        String version = artifact.getVersion();
        if (!version.matches("[A-Za-z0-9_.,+\\[\\]()-]{1,200}") || version.contains("..")) {
            throw new IOException("Invalid plugin Maven version: " + artifact);
        }
        try { new org.eclipse.aether.util.version.GenericVersionScheme().parseVersionConstraint(version); }
        catch (org.eclipse.aether.version.InvalidVersionSpecificationException error) { throw new IOException("Invalid plugin Maven version: " + artifact, error); }
        if (!artifact.getClassifier().matches("[A-Za-z0-9_.+-]{0,200}") || artifact.getClassifier().contains("..")) {
            throw new IOException("Invalid plugin Maven classifier: " + artifact);
        }
    }

    private static void validateRepository(URI uri) throws IOException {
        String host = uri.getHost();
        boolean loopback = "127.0.0.1".equals(host) || "[::1]".equals(host) || "localhost".equalsIgnoreCase(host);
        if (host == null || uri.getUserInfo() != null || uri.getQuery() != null || uri.getFragment() != null
                || !("https".equals(uri.getScheme()) || (loopback && "http".equals(uri.getScheme())))
                || uri.getPath().contains("..") || uri.getRawPath().contains("%") || uri.getPath().contains("\\")) {
            throw new IOException("Invalid plugin Maven repository: " + uri);
        }
        host = host.toLowerCase(java.util.Locale.ROOT);
        if (host.equals("maven.org") || host.endsWith(".maven.org") || host.equals("maven.apache.org") || host.endsWith(".maven.apache.org")) {
            throw new IOException("Use Paper's configured Maven Central mirror instead of " + host);
        }
    }

    private static Path directory(Path path) throws IOException {
        Path absolute = path.toAbsolutePath().normalize();
        Path current = absolute.getRoot();
        for (Path component : absolute) {
            current = current.resolve(component);
            if (Files.isSymbolicLink(current)) throw new IOException("Library cache contains a symbolic link: " + current);
        }
        Files.createDirectories(absolute);
        return absolute.toRealPath();
    }

    private static void validateJar(Path path) throws IOException {
        if (!Files.isRegularFile(path, LinkOption.NOFOLLOW_LINKS) || Files.size(path) > MAX_BYTES) {
            throw new IOException("Invalid or oversized plugin library: " + path);
        }
        long expanded = 0;
        int entries = 0;
        byte[] buffer = new byte[8192];
        try (var jar = new java.util.jar.JarFile(path.toFile(), true)) {
            var enumeration = jar.entries();
            while (enumeration.hasMoreElements()) {
                var entry = enumeration.nextElement();
                if (++entries > 100_000) throw new IOException("Too many entries in plugin library");
                if (entry.isDirectory()) continue;
                try (InputStream input = jar.getInputStream(entry)) {
                    int count;
                    while ((count = input.read(buffer)) != -1) {
                        expanded += count;
                        if (expanded > MAX_EXPANDED_BYTES) throw new IOException("Expanded plugin library exceeds size limit");
                    }
                }
            }
        } catch (SecurityException error) {
            throw new IOException("Plugin library signature validation failed", error);
        }
        if (expanded == 0) throw new IOException("Empty plugin library: " + path);
    }

    private static String digest(Path path) throws IOException {
        MessageDigest hash = sha256();
        try (InputStream input = Files.newInputStream(path)) {
            byte[] buffer = new byte[8192];
            int count;
            while ((count = input.read(buffer)) != -1) hash.update(buffer, 0, count);
        }
        return HexFormat.of().formatHex(hash.digest());
    }

    private static MessageDigest sha256() {
        try { return MessageDigest.getInstance("SHA-256"); }
        catch (NoSuchAlgorithmException error) { throw new IllegalStateException("JVM has no SHA-256", error); }
    }

    private static boolean verified(Path path, String expected) throws IOException {
        return Files.isRegularFile(path, LinkOption.NOFOLLOW_LINKS) && Files.size(path) <= MAX_BYTES && digest(path).equals(expected);
    }

    private static final class BoundedTransportFactory implements TransporterFactory {
        private final Path cache;
        private final long deadline = System.nanoTime() + TIME_LIMIT_NANOS;
        private final AtomicInteger requests = new AtomicInteger();
        private final java.util.concurrent.atomic.AtomicLong transferred = new java.util.concurrent.atomic.AtomicLong();
        private final java.util.Set<Path> responses = java.util.concurrent.ConcurrentHashMap.newKeySet();

        private BoundedTransportFactory(Path cache) throws IOException { this.cache = directory(cache.resolve("resources")); }
        @Override public float getPriority() { return 1; }
        @Override public Transporter newInstance(RepositorySystemSession session, RemoteRepository repository) throws NoTransporterException {
            try {
                URI base = URI.create(repository.getUrl().endsWith("/") ? repository.getUrl() : repository.getUrl() + "/");
                validateRepository(base);
                return new BoundedTransport(base, this);
            } catch (IOException | IllegalArgumentException error) { throw new NoTransporterException(repository, error); }
        }

        private void check() throws IOException {
            if (System.nanoTime() > deadline || Thread.currentThread().isInterrupted()) throw new IOException("Plugin library resolution deadline exceeded");
        }

        private void invalidate() throws IOException {
            for (Path response : responses) Files.deleteIfExists(response);
        }

        private Path fetch(URI uri) throws IOException {
            check();
            if (requests.incrementAndGet() > MAX_REQUESTS) throw new IOException("Plugin library resolution request limit exceeded");
            String key = HexFormat.of().formatHex(sha256().digest(uri.toASCIIString().getBytes(java.nio.charset.StandardCharsets.UTF_8)));
            Path target = cache.resolve(key);
            Path checksum = cache.resolve(key + ".sha256");
            responses.add(target);
            responses.add(checksum);
            boolean mutable = uri.getPath().contains("maven-metadata") || uri.getPath().contains("-SNAPSHOT/");
            if (!mutable && Files.isRegularFile(checksum, LinkOption.NOFOLLOW_LINKS) && Files.size(checksum) == 64
                    && verified(target, Files.readString(checksum))) return target;
            Path staging = Files.createTempFile(cache, ".download-", ".part");
            Path hashStaging = null;
            try {
                URI location = uri;
                for (int redirect = 0; ; redirect++) {
                    validateRepository(location);
                    check();
                    HttpURLConnection connection = (HttpURLConnection) location.toURL().openConnection();
                    connection.setConnectTimeout(5_000);
                    connection.setReadTimeout(5_000);
                    connection.setInstanceFollowRedirects(false);
                    try {
                        int status = connection.getResponseCode();
                        if (status == 404) throw new FileNotFoundException(location.toString());
                        if (status >= 300 && status < 400) {
                            if (redirect == 3) throw new IOException("Too many Maven repository redirects");
                            String next = connection.getHeaderField("Location");
                            if (next == null) throw new IOException("Maven redirect has no location");
                            location = location.resolve(next);
                            continue;
                        }
                        if (status != 200) throw new IOException("Maven repository returned HTTP " + status + " for " + location);
                        if (connection.getContentLengthLong() > MAX_BYTES) throw new IOException("Plugin library download exceeds size limit");
                        long total = 0;
                        try (InputStream input = connection.getInputStream(); var output = Files.newOutputStream(staging)) {
                            byte[] buffer = new byte[8192];
                            int count;
                            while ((count = input.read(buffer)) != -1) {
                                check();
                                total += count;
                                if (total > MAX_BYTES) throw new IOException("Plugin library download exceeds size limit");
                                if (transferred.addAndGet(count) > MAX_TRANSFER_BYTES) throw new IOException("Plugin library resolution exceeds total transfer limit");
                                output.write(buffer, 0, count);
                            }
                        }
                        if (connection.getContentLengthLong() >= 0 && total != connection.getContentLengthLong()) throw new IOException("Incomplete plugin library download");
                        break;
                    } finally { connection.disconnect(); }
                }
                hashStaging = Files.createTempFile(cache, ".digest-", ".part");
                Files.writeString(hashStaging, digest(staging));
                Files.move(staging, target, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
                Files.move(hashStaging, checksum, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
                return target;
            } finally {
                Files.deleteIfExists(staging);
                if (hashStaging != null) Files.deleteIfExists(hashStaging);
            }
        }
    }

    private static final class BoundedTransport extends AbstractTransporter {
        private final URI base;
        private final BoundedTransportFactory factory;
        private BoundedTransport(URI base, BoundedTransportFactory factory) { this.base = base; this.factory = factory; }
        private Path resource(URI relative) throws IOException {
            String path = relative.toASCIIString();
            if (relative.isAbsolute() || path.startsWith("/") || path.contains("..") || path.contains("%") || path.contains("\\")) {
                throw new IOException("Invalid Maven artifact path: " + relative);
            }
            return factory.fetch(base.resolve(relative));
        }
        @Override public int classify(Throwable error) { return error instanceof FileNotFoundException ? ERROR_NOT_FOUND : ERROR_OTHER; }
        @Override protected void implPeek(PeekTask task) throws Exception { resource(task.getLocation()); }
        @Override protected void implGet(GetTask task) throws Exception {
            Path path = resource(task.getLocation());
            utilGet(task, Files.newInputStream(path), true, Files.size(path), false);
        }
        @Override protected void implPut(PutTask task) throws IOException { throw new IOException("Plugin library repositories are read-only"); }
        @Override protected void implClose() {}
    }
}
