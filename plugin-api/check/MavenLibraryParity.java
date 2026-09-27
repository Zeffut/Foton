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
        HttpServer server = HttpServer.create(new InetSocketAddress("127.0.0.1", 0), 0);
        server.createContext("/", exchange -> {
            byte[] bytes = resources.get(exchange.getRequestURI().getPath());
            exchange.sendResponseHeaders(bytes == null ? 404 : 200, bytes == null ? -1 : bytes.length);
            if (bytes != null) exchange.getResponseBody().write(bytes);
            exchange.close();
        });
        server.start();
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
        } finally { server.stop(0); }
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
