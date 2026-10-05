package foton;

import com.sun.net.httpserver.HttpServer;
import java.io.ByteArrayOutputStream;
import java.net.InetSocketAddress;
import java.net.URL;
import java.net.URLClassLoader;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HexFormat;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.jar.JarEntry;
import java.util.jar.JarOutputStream;
import org.eclipse.aether.artifact.DefaultArtifact;
import org.eclipse.aether.graph.Dependency;
import org.eclipse.aether.repository.RemoteRepository;

/** A real local Maven repository: POM graphs, checksums and JARs travel over HTTP. */
public final class PluginLibraries {
    private PluginLibraries() {}
    public static void main(String[] args) throws Exception { check(); }

    public static void check() throws Exception {
        Class<?> resolver = Class.forName("foton.PluginLibraryResolver");
        Path work = Files.createTempDirectory("foton-maven-check.");
        Map<String, byte[]> repository = new ConcurrentHashMap<>();
        HttpServer server = HttpServer.create(new InetSocketAddress("127.0.0.1", 0), 0);
        server.createContext("/", exchange -> {
            if (exchange.getRequestURI().getPath().startsWith("/oversized/")) {
                exchange.sendResponseHeaders(200, 65L * 1024 * 1024);
                exchange.close();
                return;
            }
            if (exchange.getRequestURI().getPath().startsWith("/redirect/")) {
                exchange.getResponseHeaders().add("Location", "https://repo.maven.apache.org/maven2/blocked");
                exchange.sendResponseHeaders(302, -1);
                exchange.close();
                return;
            }
            byte[] content = repository.get(exchange.getRequestURI().getPath());
            exchange.sendResponseHeaders(content == null ? 404 : 200, content == null ? -1 : content.length);
            if (content != null) exchange.getResponseBody().write(content);
            exchange.close();
        });
        server.start();
        String address = "http://127.0.0.1:" + server.getAddress().getPort() + "/";
        try {
            artifact(work, repository, "child", "1", "");
            artifact(work, repository, "child", "2", "");
            artifact(work, repository, "parent", "1", dependency("child", "1"));
            publish(repository, "/test/child/maven-metadata.xml", ("<metadata><groupId>test</groupId><artifactId>child</artifactId>"
                + "<versioning><latest>2</latest><release>2</release><versions><version>1</version><version>2</version></versions>"
                + "<lastUpdated>20260927000000</lastUpdated></versioning></metadata>").getBytes(StandardCharsets.UTF_8));
            List<Dependency> dependencies = List.of(dep("parent", "1"), dep("child", "2"));
            List<RemoteRepository> repositories = List.of(new RemoteRepository.Builder("fixture", "default", address).build());
            List<URL> urls = resolve(resolver, work, dependencies, repositories);
            expect(urls.size() == 2, "nearest direct dependency must replace conflicting transitive version");
            try (URLClassLoader loader = new URLClassLoader(urls.toArray(URL[]::new), null)) {
                expect(resource(loader, "child.txt").equals("2"), "conflict winner must be direct child 2");
                expect(resource(loader, "parent.txt").equals("1"), "root artifact must be resolved");
            }
            List<URL> transitive = resolve(resolver, work, List.of(dep("parent", "1")), repositories);
            expect(transitive.size() == 2, "POM child must enter the resolved classpath");
            List<URL> ranged = resolve(resolver, work, List.of(dep("child", "[1,3)")), repositories);
            try (URLClassLoader loader = new URLClassLoader(ranged.toArray(URL[]::new), null)) {
                expect(resource(loader, "child.txt").equals("2"), "Maven version range must use repository metadata");
            }
            try (URLClassLoader isolated = new URLClassLoader(new URL[0], null)) {
                expect(isolated.getResource("child.txt") == null, "libraries must not leak into unrelated loaders");
            }
            Path corrupt = Path.of(urls.getFirst().toURI());
            Files.writeString(corrupt, "corrupt");
            List<URL> repaired = resolve(resolver, work, dependencies, repositories);
            try (URLClassLoader loader = new URLClassLoader(repaired.toArray(URL[]::new), null)) {
                expect(resource(loader, "parent.txt").equals("1"), "corrupt published cache must be repaired");
            }
            String cachedResource = HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256")
                .digest((address + "test/parent/1/parent-1.jar").getBytes(StandardCharsets.UTF_8)));
            Files.writeString(work.resolve("resources").resolve(cachedResource), "corrupt cached download");
            resolve(resolver, work, dependencies, repositories);
            expect(Files.size(work.resolve("resources").resolve(cachedResource)) > 100,
                "corrupt transport cache must be downloaded and verified again");
            rejects(() -> resolve(resolver, work, List.of(new Dependency(new DefaultArtifact("test:../escape:1"), null)), repositories), "coordinate traversal");
            rejects(() -> resolve(resolver, work, dependencies, List.of(new RemoteRepository.Builder("bad", "default", address + "../").build())), "repository traversal");
            rejects(() -> resolve(resolver, work, dependencies, List.of(new RemoteRepository.Builder("central", "default", "https://repo.maven.apache.org/maven2").build())), "direct Central CDN");
            rejects(() -> resolve(resolver, work, List.of(dep("absent", "1")), repositories), "missing remote artifact");
            rejects(() -> resolve(resolver, work, dependencies, List.of(new RemoteRepository.Builder("size", "default", address + "oversized/").build())), "oversized response");
            rejects(() -> resolve(resolver, work, dependencies, List.of(new RemoteRepository.Builder("redirect", "default", address + "redirect/").build())), "redirect to direct Central");
            artifact(work, repository, "badchecksum", "1", "");
            repository.put("/test/badchecksum/1/badchecksum-1.jar.sha1", "0000000000000000000000000000000000000000".getBytes(StandardCharsets.US_ASCII));
            rejects(() -> resolve(resolver, work, List.of(dep("badchecksum", "1")), repositories), "repository checksum mismatch");
            reviewRegressions(resolver, work, repository, repositories, address, Path.of(repaired.getFirst().toURI()));
            server.stop(0);
            rejects(() -> resolve(resolver, work, List.of(dep("unreachable", "1")), repositories), "unreachable repository");
        } finally {
            server.stop(0);
            try (var paths = Files.walk(work)) {
                for (Path path : paths.sorted(Comparator.reverseOrder()).toList()) Files.deleteIfExists(path);
            }
        }
    }

    private static void reviewRegressions(Class<?> resolver, Path work, Map<String, byte[]> repository,
            List<RemoteRepository> repositories, String address, Path localLibrary) throws Exception {
        List<Throwable> failures = new ArrayList<>();
        try {
            artifact(work, repository, "profile", "1", "");
            String pom = "<project><modelVersion>4.0.0</modelVersion><groupId>test</groupId><artifactId>profile</artifactId><version>1</version>"
                + "<profiles><profile><id>java</id><activation><jdk>[21,)</jdk></activation><dependencies>"
                + dependency("child", "${foton.maven.child}") + "</dependencies></profile>"
                + "<profile><id>property</id><activation><property><name>foton.maven.child</name><value>2</value></property></activation>"
                + "<dependencies>" + dependency("parent", "1") + "</dependencies></profile></profiles></project>";
            publish(repository, "/test/profile/1/profile-1.pom", pom.getBytes(StandardCharsets.UTF_8));
            String previous = System.getProperty("foton.maven.child");
            System.setProperty("foton.maven.child", "2");
            try {
                List<URL> urls = resolve(resolver, work, List.of(dep("profile", "1")), repositories);
                try (URLClassLoader loader = new URLClassLoader(urls.toArray(URL[]::new), null)) {
                    expect(resource(loader, "child.txt").equals("2"), "JDK profile and JVM property interpolation must activate child 2");
                    expect(resource(loader, "parent.txt").equals("1"), "JVM property must activate the parent dependency profile");
                }
            } finally {
                if (previous == null) System.clearProperty("foton.maven.child");
                else System.setProperty("foton.maven.child", previous);
            }
        } catch (Exception | AssertionError error) { failures.add(error); }
        try {
            String path = "/test/badchecksum/1/badchecksum-1.jar";
            publish(repository, path, repository.get(path));
            expect(resolve(resolver, work, List.of(dep("badchecksum", "1")), repositories).size() == 1,
                "corrected mirror must succeed on the next resolution with the same cache");
        } catch (Exception | AssertionError error) { failures.add(error); }
        try { PluginLibraryHostChecks.check(work, address, localLibrary); }
        catch (Exception | AssertionError error) { failures.add(error); }
        if (!failures.isEmpty()) {
            AssertionError error = new AssertionError("library review regressions: " + failures.size());
            failures.forEach(error::addSuppressed);
            throw error;
        }
    }

    @SuppressWarnings("unchecked")
    private static List<URL> resolve(Class<?> type, Path cache, List<Dependency> dependencies, List<RemoteRepository> repositories) throws Exception {
        try {
            return (List<URL>) type.getMethod("resolve", Path.class, List.class, List.class).invoke(null, cache, dependencies, repositories);
        } catch (java.lang.reflect.InvocationTargetException error) {
            if (error.getCause() instanceof Exception cause) throw cause;
            throw error;
        }
    }

    private static String resource(ClassLoader loader, String name) throws Exception {
        try (var stream = loader.getResourceAsStream(name)) {
            if (stream == null) throw new AssertionError("missing " + name);
            return new String(stream.readAllBytes(), StandardCharsets.UTF_8);
        }
    }

    private static Dependency dep(String artifact, String version) {
        return new Dependency(new DefaultArtifact("test:" + artifact + ":" + version), null);
    }

    private static String dependency(String artifact, String version) {
        return "<dependency><groupId>test</groupId><artifactId>" + artifact + "</artifactId><version>" + version + "</version></dependency>";
    }

    private static void artifact(Path work, Map<String, byte[]> repository, String artifact, String version, String dependencies) throws Exception {
        String prefix = "/test/" + artifact + "/" + version + "/" + artifact + "-" + version;
        String pom = "<project><modelVersion>4.0.0</modelVersion><groupId>test</groupId><artifactId>" + artifact
            + "</artifactId><version>" + version + "</version><dependencies>" + dependencies + "</dependencies></project>";
        publish(repository, prefix + ".pom", pom.getBytes(StandardCharsets.UTF_8));
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        try (JarOutputStream jar = new JarOutputStream(bytes)) {
            jar.putNextEntry(new JarEntry(artifact + ".txt"));
            jar.write(version.getBytes(StandardCharsets.UTF_8));
            jar.closeEntry();
            Path classes = Files.createDirectories(work.resolve("compiled-" + artifact + version));
            String name = artifact.equals("child") ? "Child" : "Parent";
            Path source = classes.resolve(name + ".java");
            Files.writeString(source, "package fixturelib; public class " + name + " { public static String value() { return \"" + version + "\"; } }");
            int status = javax.tools.ToolProvider.getSystemJavaCompiler().run(null, null, null, "-d", classes.toString(), source.toString());
            expect(status == 0, "library fixture compiles");
            jar.putNextEntry(new JarEntry("fixturelib/" + name + ".class"));
            jar.write(Files.readAllBytes(classes.resolve("fixturelib/" + name + ".class")));
            jar.closeEntry();
        }
        publish(repository, prefix + ".jar", bytes.toByteArray());
    }

    private static void publish(Map<String, byte[]> repository, String path, byte[] bytes) throws Exception {
        repository.put(path, bytes);
        repository.put(path + ".sha1", HexFormat.of().formatHex(MessageDigest.getInstance("SHA-1").digest(bytes)).getBytes(StandardCharsets.US_ASCII));
    }

    private interface Checked { void run() throws Exception; }
    private static void rejects(Checked operation, String reason) throws Exception {
        try { operation.run(); } catch (Exception expected) { return; }
        throw new AssertionError("must reject " + reason);
    }
    private static void expect(boolean value, String reason) { if (!value) throw new AssertionError(reason); }
}
