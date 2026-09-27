import com.sun.net.httpserver.HttpServer;
import io.papermc.paper.plugin.loader.library.impl.MavenLibraryResolver;
import java.io.ByteArrayOutputStream;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HexFormat;
import java.util.List;
import java.util.Map;
import java.util.jar.JarEntry;
import java.util.jar.JarFile;
import java.util.jar.JarOutputStream;
import org.eclipse.aether.artifact.DefaultArtifact;
import org.eclipse.aether.graph.Dependency;
import org.eclipse.aether.repository.RemoteRepository;

/** Runs unchanged against either the actual Paper API or Foton; use a temporary cwd. */
public final class MavenLibraryParity {
    public static void main(String[] args) throws Exception {
        Map<String, byte[]> resources = new HashMap<>();
        artifact(resources, "child", "1", "");
        artifact(resources, "child", "2", "");
        artifact(resources, "parent", "1", "<dependency><groupId>fixture</groupId><artifactId>child</artifactId><version>1</version></dependency>");
        artifact(resources, "profile", "1", "");
        put(resources, "/fixture/profile/1/profile-1.pom", ("<project><modelVersion>4.0.0</modelVersion><groupId>fixture</groupId>"
            + "<artifactId>profile</artifactId><version>1</version><profiles><profile><id>java</id><activation><jdk>[21,)</jdk></activation>"
            + "<dependencies><dependency><groupId>fixture</groupId><artifactId>child</artifactId><version>${foton.maven.child}</version>"
            + "</dependency></dependencies></profile><profile><id>property</id><activation><property><name>foton.maven.child</name>"
            + "<value>2</value></property></activation><dependencies><dependency><groupId>fixture</groupId><artifactId>parent</artifactId>"
            + "<version>1</version></dependency></dependencies></profile></profiles></project>").getBytes(StandardCharsets.UTF_8));
        artifact(resources, "retry", "1", "");
        byte[] validChecksum = resources.put("/fixture/retry/1/retry-1.jar.sha1", "0000000000000000000000000000000000000000".getBytes(StandardCharsets.US_ASCII));
        HttpServer server = HttpServer.create(new InetSocketAddress("127.0.0.1", 0), 0);
        server.createContext("/", exchange -> {
            byte[] bytes = resources.get(exchange.getRequestURI().getPath());
            exchange.sendResponseHeaders(bytes == null ? 404 : 200, bytes == null ? -1 : bytes.length);
            if (bytes != null) exchange.getResponseBody().write(bytes);
            exchange.close();
        });
        server.start();
        String previousChild = System.getProperty("foton.maven.child");
        String previousCentral = System.getProperty("org.bukkit.plugin.java.LibraryLoader.centralURL");
        System.setProperty("org.bukkit.plugin.java.LibraryLoader.centralURL", "http://127.0.0.1:" + server.getAddress().getPort());
        System.setProperty("foton.maven.child", "2");
        try {
            var repository = new RemoteRepository.Builder("fixture", "default", "http://127.0.0.1:" + server.getAddress().getPort()).build();
            var transitive = new MavenLibraryResolver();
            transitive.addRepository(repository);
            transitive.addDependency(new Dependency(new DefaultArtifact("fixture:parent:1"), null));
            System.out.println("transitive=" + resolve(transitive));
            var conflict = new MavenLibraryResolver();
            conflict.addRepository(repository);
            conflict.addDependency(new Dependency(new DefaultArtifact("fixture:parent:1"), null));
            conflict.addDependency(new Dependency(new DefaultArtifact("fixture:child:2"), null));
            System.out.println("conflict=" + resolve(conflict));
            var profile = new MavenLibraryResolver();
            profile.addRepository(repository);
            profile.addDependency(new Dependency(new DefaultArtifact("fixture:profile:1"), null));
            System.out.println("profile=" + resolve(profile));
            var retry = new MavenLibraryResolver();
            retry.addRepository(repository);
            retry.addDependency(new Dependency(new DefaultArtifact("fixture:retry:1"), null));
            try {
                resolve(retry);
                throw new AssertionError("bad checksum accepted");
            } catch (io.papermc.paper.plugin.loader.library.LibraryLoadingException expected) {
                System.out.println("checksum=rejected");
            }
            resources.put("/fixture/retry/1/retry-1.jar.sha1", validChecksum);
            var corrected = new MavenLibraryResolver();
            corrected.addRepository(repository);
            corrected.addDependency(new Dependency(new DefaultArtifact("fixture:retry:1"), null));
            System.out.println("corrected=" + resolve(corrected));
            if (args.length > 0 && args[0].equals("legacy")) {
                List<Path> customPaths = new ArrayList<>();
                transitive.register(customPaths::add);
                var description = new org.bukkit.plugin.PluginDescriptionFile(new java.io.StringReader(
                    "name: Combined\nversion: 1\nmain: fixture.Main\nlibraries: [fixture:child:2]\n"));
                Class<?> type = Class.forName("org.bukkit.plugin.java.LibraryLoader");
                Object loader = type.getConstructor(java.util.logging.Logger.class).newInstance(java.util.logging.Logger.getLogger("parity"));
                try (var classpath = (java.net.URLClassLoader) type.getMethod("createLoader", org.bukkit.plugin.PluginDescriptionFile.class, List.class)
                        .invoke(loader, description, customPaths)) {
                    var values = java.util.Collections.list(classpath.getResources("value.txt"));
                    try (var input = values.get(1).openStream()) {
                        System.out.println("legacy-order=" + new String(input.readAllBytes(), StandardCharsets.UTF_8));
                    }
                }
            }
        } finally {
            server.stop(0);
            if (previousChild == null) System.clearProperty("foton.maven.child");
            else System.setProperty("foton.maven.child", previousChild);
            if (previousCentral == null) System.clearProperty("org.bukkit.plugin.java.LibraryLoader.centralURL");
            else System.setProperty("org.bukkit.plugin.java.LibraryLoader.centralURL", previousCentral);
        }
    }

    private static List<String> resolve(MavenLibraryResolver resolver) throws Exception {
        List<Path> paths = new ArrayList<>();
        resolver.register(paths::add);
        List<String> result = new ArrayList<>();
        for (Path path : paths) {
            try (JarFile jar = new JarFile(path.toFile()); var input = jar.getInputStream(jar.getJarEntry("value.txt"))) {
                result.add(new String(input.readAllBytes(), StandardCharsets.UTF_8));
            }
        }
        return result;
    }

    private static void artifact(Map<String, byte[]> resources, String name, String version, String dependencies) throws Exception {
        String base = "/fixture/" + name + "/" + version + "/" + name + "-" + version;
        put(resources, base + ".pom", ("<project><modelVersion>4.0.0</modelVersion><groupId>fixture</groupId><artifactId>"
            + name + "</artifactId><version>" + version + "</version><dependencies>" + dependencies + "</dependencies></project>").getBytes(StandardCharsets.UTF_8));
        var bytes = new ByteArrayOutputStream();
        try (JarOutputStream jar = new JarOutputStream(bytes)) {
            jar.putNextEntry(new JarEntry("value.txt"));
            jar.write((name + ":" + version).getBytes(StandardCharsets.UTF_8));
            jar.closeEntry();
        }
        put(resources, base + ".jar", bytes.toByteArray());
    }

    private static void put(Map<String, byte[]> resources, String path, byte[] bytes) throws Exception {
        resources.put(path, bytes);
        resources.put(path + ".sha1", HexFormat.of().formatHex(MessageDigest.getInstance("SHA-1").digest(bytes)).getBytes(StandardCharsets.US_ASCII));
    }
}
