package foton;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.jar.JarEntry;
import java.util.jar.JarOutputStream;

/** Real loader bytecode; an optional official API classpath also checks Paper's binary ABI. */
final class PluginLibraryHostChecks {
    private PluginLibraryHostChecks() {}

    static void check(Path work, String repository, Path localLibrary) throws Exception {
        String previousMirror = System.getProperty("org.bukkit.plugin.java.LibraryLoader.centralURL");
        System.setProperty("org.bukkit.plugin.java.LibraryLoader.centralURL", repository);
        org.bukkit.Server previousServer = org.bukkit.Bukkit.getServer();
        var serverField = org.bukkit.Bukkit.class.getDeclaredField("server");
        serverField.setAccessible(true);
        org.bukkit.Server delegate = previousServer == null ? new FotonServer() : previousServer;
        serverField.set(null, java.lang.reflect.Proxy.newProxyInstance(PluginLibraryHostChecks.class.getClassLoader(),
            new Class<?>[] {org.bukkit.Server.class}, (proxy, method, arguments) -> {
                if (method.getName().equals("getMinecraftVersion")) return "26.2";
                try { return method.invoke(delegate, arguments); }
                catch (java.lang.reflect.InvocationTargetException error) { throw error.getCause(); }
            }));
        try {
            Path plugins = Files.createDirectories(work.resolve("host-plugins"));
            fixture(plugins, "JsonLoader", true, "loader: fixture.Loader\n", repository, localLibrary);
            fixture(plugins, "LegacyLibraries", false, "libraries: [test:parent:1]\n", null, null);
            fixture(plugins, "LegacyLoader", false, "paper-plugin-loader: fixture.Loader\n", repository, localLibrary);
            fixture(plugins, "CombinedLoader", false, "paper-plugin-loader: fixture.Loader\nlibraries: [test:child:2]\n", repository, localLibrary);
            fixture(plugins, "Isolated", false, "", null, null);
            expect(PluginHost.loadAll(plugins.toString()) == 5, "Paper JSON loader, legacy libraries and legacy loader must load");
            for (String name : java.util.List.of("JsonLoader", "LegacyLibraries", "LegacyLoader", "CombinedLoader")) {
                ClassLoader loader = PluginHost.byName(name).getClass().getClassLoader();
                Class<?> child = Class.forName("fixturelib.Child", true, loader);
                expect(child.getMethod("value").invoke(null).equals("1"), name + " resolves transitive child");
            }
            ClassLoader isolated = PluginHost.byName("Isolated").getClass().getClassLoader();
            try {
                Class.forName("fixturelib.Child", true, isolated);
                throw new AssertionError("library class leaked across plugin loaders");
            } catch (ClassNotFoundException expected) { }
            expect(PluginHost.byName("JsonLoader").getClass().getClassLoader().getResource("parent.txt") != null,
                "JarLibrary resource available to plugin");
            java.util.List<Path> paths = new java.util.ArrayList<>();
            new io.papermc.paper.plugin.loader.library.impl.JarLibrary(localLibrary).register(paths::add);
            expect(paths.size() == 1, "absolute local library is accepted");
            try {
                new io.papermc.paper.plugin.loader.library.impl.JarLibrary(localLibrary.getParent().resolve("../" + localLibrary.getParent().getFileName() + "/" + localLibrary.getFileName())).register(paths::add);
                throw new AssertionError("local path traversal accepted");
            } catch (io.papermc.paper.plugin.loader.library.LibraryLoadingException expected) { }
            PluginHost.disableAll();
            Path failed = Files.createDirectories(work.resolve("failed-plugins"));
            fixture(failed, "BrokenLibrary", false, "libraries: [test:missing:1]\n", null, null);
            expect(PluginHost.loadAll(failed.toString()) == 0, "unresolved library prevents publication");
            expect(PluginHost.byName("BrokenLibrary") == null, "failed library leaves no registered plugin");
            System.out.println("Plugin libraries checked: transitive graph, conflict, cache, isolation, JSON loader, legacy loader/libraries, local JAR, failures");
        } finally {
            PluginHost.disableAll();
            serverField.set(null, previousServer);
            if (previousMirror == null) System.clearProperty("org.bukkit.plugin.java.LibraryLoader.centralURL");
            else System.setProperty("org.bukkit.plugin.java.LibraryLoader.centralURL", previousMirror);
        }
    }

    private static void fixture(Path directory, String name, boolean paper, String metadata,
            String repository, Path localLibrary) throws Exception {
        Path sources = Files.createDirectories(directory.resolve(name + "-sources"));
        Path classes = Files.createDirectories(sources.resolve("classes"));
        Path main = sources.resolve("Main.java");
        Files.writeString(main, "package fixture; public class Main extends org.bukkit.plugin.java.JavaPlugin { }");
        java.util.List<String> arguments = new java.util.ArrayList<>(java.util.List.of("-cp",
            System.getProperty("foton.paper.api.classpath", "") + java.io.File.pathSeparator + System.getProperty("java.class.path"),
            "-d", classes.toString(), main.toString()));
        if (repository != null) {
            Path loader = sources.resolve("Loader.java");
            Files.writeString(loader, """
                package fixture;
                public class Loader implements io.papermc.paper.plugin.loader.PluginLoader {
                    public void classloader(io.papermc.paper.plugin.loader.PluginClasspathBuilder builder) {
                        builder.getContext().getLogger().info("library fixture loader");
                        try (var input = getClass().getResourceAsStream("/paper-libraries.json")) {
                            var root = com.google.gson.JsonParser.parseReader(new java.io.InputStreamReader(input)).getAsJsonObject();
                            var resolver = new io.papermc.paper.plugin.loader.library.impl.MavenLibraryResolver();
                            for (var entry : root.getAsJsonObject("repositories").entrySet())
                                resolver.addRepository(new org.eclipse.aether.repository.RemoteRepository.Builder(entry.getKey(), "default", entry.getValue().getAsString()).build());
                            for (var dependency : root.getAsJsonArray("loader-dependencies"))
                                resolver.addDependency(new org.eclipse.aether.graph.Dependency(new org.eclipse.aether.artifact.DefaultArtifact(dependency.getAsString()), null));
                            builder.addLibrary(resolver);
                            builder.addLibrary(new io.papermc.paper.plugin.loader.library.impl.JarLibrary(java.nio.file.Path.of(root.get("local").getAsString())));
                        } catch (Exception error) { throw new RuntimeException(error); }
                    }
                }
                """);
            arguments.add(loader.toString());
            var json = new com.google.gson.JsonObject();
            var repositories = new com.google.gson.JsonObject();
            repositories.addProperty("fixture", repository);
            json.add("repositories", repositories);
            var dependencies = new com.google.gson.JsonArray();
            dependencies.add("test:parent:1");
            json.add("loader-dependencies", dependencies);
            // If the host also applies its legacy adapter, this fails before
            // any download. A declared loader owns the JSON schema exclusively.
            var hostTrap = new com.google.gson.JsonArray();
            hostTrap.add("..:host-must-not-resolve:1");
            json.add("dependencies", hostTrap);
            json.addProperty("local", localLibrary.toString());
            Files.writeString(classes.resolve("paper-libraries.json"), json.toString());
        }
        if (name.equals("Isolated")) {
            Files.writeString(classes.resolve("paper-libraries.json"),
                "{\"repositories\":{},\"dependencies\":[]}");
        }
        int result = javax.tools.ToolProvider.getSystemJavaCompiler().run(null, null, null, arguments.toArray(String[]::new));
        expect(result == 0, "plugin fixture compilation");
        Files.writeString(classes.resolve(paper ? "paper-plugin.yml" : "plugin.yml"),
            "name: " + name + "\nversion: 1\nmain: fixture.Main\napi-version: '26.2'\n" + metadata);
        try (JarOutputStream jar = new JarOutputStream(Files.newOutputStream(directory.resolve(name + ".jar"))); var files = Files.walk(classes)) {
            for (Path path : files.filter(Files::isRegularFile).toList()) {
                jar.putNextEntry(new JarEntry(classes.relativize(path).toString().replace('\\', '/')));
                Files.copy(path, jar);
                jar.closeEntry();
            }
        }
    }

    private static void expect(boolean value, String message) { if (!value) throw new AssertionError(message); }
}
