package foton;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.jar.JarEntry;
import java.util.jar.JarOutputStream;

/** Real, isolated fixture jars: no fixture classes are on the host classpath. */
public final class PaperLoading {
    private PaperLoading() {}

    public static void main(String[] args) throws Exception { check(); }

    public static void check() throws Exception {
        Path root = Files.createTempDirectory("foton-paper-loading-");
        org.bukkit.Server previous = org.bukkit.Bukkit.getServer();
        org.bukkit.Server delegate = previous == null ? new FotonServer() : previous;
        var serverField = org.bukkit.Bukkit.class.getDeclaredField("server");
        serverField.setAccessible(true);
        serverField.set(null, (org.bukkit.Server) java.lang.reflect.Proxy.newProxyInstance(
            PaperLoading.class.getClassLoader(), new Class<?>[] {org.bukkit.Server.class},
            (proxy, method, arguments) -> {
                if (method.getName().equals("getMinecraftVersion")) return "26.2";
                try { return method.invoke(delegate, arguments); }
                catch (java.lang.reflect.InvocationTargetException error) { throw error.getCause(); }
            }));
        try {
            Path collision = root.resolve("collision");
            fixture(collision, "Alpha", false, "provides: [Zulu, Shared]\n", null);
            fixture(collision, "Zulu", false, "provides: [Shared]\n", null);
            equal(PluginHost.loadAll(collision.toString()), 2, "real name not suppressed by alias");
            equal(PluginHost.byName("Zulu").getName(), "Zulu", "real name wins alias collision");
            equal(PluginHost.byName("Shared").getName(), "Alpha", "selected alias independent of startup order");
            PluginHost.disableAll();
            Path competing = root.resolve("competing");
            fixture(competing, "AliasAlpha", false, "provides: [Competing]\n", null);
            fixture(competing, "AliasZulu", false, "load: STARTUP\nprovides: [Competing]\n", null);
            equal(PluginHost.loadAll(competing.toString()), 2, "competing aliases both load");
            equal(PluginHost.byName("Competing").getName(), "AliasAlpha", "graph alias selection survives startup ordering");
            PluginHost.disableAll();
            descriptorsAndPhases();
            System.setProperty("foton.paper.order", "");
            Path mixed = root.resolve("mixed");
            fixture(mixed, "AlphaConsumer", true,
                "dependencies:\n  server:\n    ServiceAlias:\n      load: BEFORE\n", null);
            fixture(mixed, "ZuluProvider", false, "provides: [ServiceAlias]\n", null);
            equal(PluginHost.loadAll(mixed.toString()), 2, "provider alias satisfies Paper dependency");
            equal(PluginHost.byName("ServiceAlias"), PluginHost.byName("ZuluProvider"), "alias lookup");
            equal(order(), List.of("ZuluProvider", "AlphaConsumer"), "mixed construction order");
            equal(System.getProperty("foton.paper.order"),
                "construct:ZuluProvider;construct:AlphaConsumer;enable:ZuluProvider;enable:AlphaConsumer;",
                "constructor and enable ordering");
            equal(Class.forName("fixture.ZuluProvider", false, PluginHost.byName("AlphaConsumer").getClass().getClassLoader()),
                PluginHost.byName("ZuluProvider").getClass(), "joined provider alias classpath");
            PluginHost.disableAll();

            Path after = root.resolve("after");
            fixture(after, "First", true,
                "dependencies:\n  server:\n    Last:\n      load: AFTER\n      join-classpath: false\n", null);
            fixture(after, "Last", false, "", null);
            equal(PluginHost.loadAll(after.toString()), 2, "required AFTER is presence, not enable precedence");
            equal(order(), List.of("First", "Last"), "Paper AFTER order");
            try {
                Class.forName("fixture.Last", false, PluginHost.byName("First").getClass().getClassLoader());
                throw new AssertionError("join-classpath:false leaked provider class");
            } catch (ClassNotFoundException expected) {
                // Ordering and required presence must not grant class visibility.
            }
            PluginHost.disableAll();

            Path omit = root.resolve("omit");
            fixture(omit, "OmitFirst", true,
                "dependencies:\n  server:\n    OmitLast: {}\n    OptionalMissing:\n      required: false\n", null);
            fixture(omit, "OmitLast", false, "", null);
            equal(PluginHost.loadAll(omit.toString()), 2, "OMIT does not impose dependency-first construction");
            equal(Class.forName("fixture.OmitLast", false, PluginHost.byName("OmitFirst").getClass().getClassLoader()),
                PluginHost.byName("OmitLast").getClass(), "OMIT classpath becomes available after publication");
            PluginHost.disableAll();

            Path both = root.resolve("both");
            fixture(both, "Modern", true, "", "name: WrongLegacy\nversion: 1\nmain: missing.Main\n");
            equal(PluginHost.loadAll(both.toString()), 1, "Paper descriptor wins");
            equal(PluginHost.byName("WrongLegacy"), null, "legacy descriptor not merged");
            PluginHost.disableAll();

            Path cycle = root.resolve("cycle");
            fixture(cycle, "CycleA", false, "depend: [CycleB]\ncommands:\n  rejected-command: {}\npermissions:\n  rejected.permission: {}\n", null);
            fixture(cycle, "CycleB", false, "depend: [CycleA]\n", null);
            equal(PluginHost.loadAll(cycle.toString()), 2, "hard cycle ordering recovered");
            PluginHost.disableAll();
            Path rejected = root.resolve("rejected");
            fixture(rejected, "Rejected", false, "depend: [Missing]\ncommands:\n  rejected-command: {}\npermissions:\n  rejected.permission: {}\n", null);
            equal(PluginHost.loadAll(rejected.toString()), 0, "missing dependency rejected");
            equal(PluginHost.all().length, 0, "rejected graph has no plugins");
            var loaders = PluginHost.class.getDeclaredField("pluginLoaders");
            loaders.setAccessible(true);
            equal(((java.util.Map<?, ?>) loaders.get(null)).size(), 0, "rejected graph has no loaders");
            equal(CommandMap.get("rejected-command"), null, "rejected graph has no commands");
            equal(PermissionRegistry.get("rejected.permission"), null, "rejected graph has no permissions");

            Path startup = root.resolve("startup");
            fixture(startup, "AlphaPost", false, "", null);
            fixture(startup, "ZuluStartup", false, "load: STARTUP\n", null);
            equal(PluginHost.loadAll(startup.toString()), 2, "startup fixtures load");
            equal(order(), List.of("ZuluStartup", "AlphaPost"), "STARTUP order");
            PluginHost.disableAll();

            Path unsupported = root.resolve("unsupported");
            fixture(unsupported, "NeedsBootstrap", true, "bootstrapper: fixture.Bootstrap\n", null);
            equal(PluginHost.loadAll(unsupported.toString()), 0, "bootstrap must not be silently skipped");
            equal(PluginHost.all().length, 0, "unsupported bootstrap not published");
            Path future = root.resolve("future");
            fixture(future, "Future", true, "", null, "99.99");
            equal(PluginHost.loadAll(future.toString()), 0, "host rejects future API version before publication");
            equal(PluginHost.all().length, 0, "future version has no published plugins");
            System.out.println("Paper loading checked: alias, mixed graph, AFTER, both descriptors, cycle, STARTUP, bootstrap rejection");
        } finally {
            System.clearProperty("foton.paper.order");
            PluginHost.disableAll();
            serverField.set(null, previous);
            try (var paths = Files.walk(root)) {
                for (Path path : paths.sorted(java.util.Comparator.reverseOrder()).toList()) {
                    Files.deleteIfExists(path);
                }
            }
        }
    }

    private static void descriptorsAndPhases() throws Exception {
        var legacy = new org.bukkit.plugin.PluginDescriptionFile(new java.io.StringReader(
            "name: Legacy\nversion: 1\nmain: fixture.Legacy\nload: STARTUP\nprovides: [Alias]\n"
                + "libraries: [org.example:library:1.0]\n"));
        equal(legacy.getLibraries(), List.of("org.example:library:1.0"), "legacy libraries retained");
        equal(legacy.getLoad(), org.bukkit.plugin.PluginLoadOrder.STARTUP, "legacy startup retained");
        var skippedLibraries = new org.bukkit.plugin.PluginDescriptionFile(new java.io.StringReader(
            "name: Skip\nversion: 1\nmain: fixture.Skip\nlibraries: [org.example:library:1.0]\npaper-skip-libraries: true\n"));
        equal(skippedLibraries.getLibraries(), List.of(), "Paper skip-libraries clears resolution list");
        var permissions = paper("Permissions", "default-permission: false\npermissions:\n  fixture.permission: {}\n");
        equal(permissions.getPermissions().get(0).getDefault(), org.bukkit.permissions.PermissionDefault.FALSE,
            "Paper permission inherits descriptor default");
        equal(paper("DefaultPermissions", "permissions:\n  fixture.permission: {}\n").getPermissions().get(0).getDefault(),
            org.bukkit.permissions.PermissionDefault.OP, "Paper default permission is op");
        var first = paper("FirstPhase", "bootstrapper: fixture.Boot\ndependencies:\n  bootstrap:\n    LastPhase:\n      load: BEFORE\n  server:\n    LastPhase:\n      load: AFTER\n      join-classpath: false\n");
        var last = paper("LastPhase", "bootstrapper: fixture.Boot\n");
        var graph = new PluginDependencyGraph(List.of(first, last));
        equal(graph.order(PaperPluginDescriptor.Phase.BOOTSTRAP), List.of("lastphase", "firstphase"), "bootstrap order");
        equal(graph.order(PaperPluginDescriptor.Phase.SERVER), List.of("firstphase", "lastphase"), "independent server order");
        equal(PluginDependencyGraph.visible(first, PaperPluginDescriptor.Phase.SERVER), List.of(), "bootstrap visibility not inherited");
        var broken = paper("Broken", "dependencies:\n  server:\n    Missing: {}\n");
        equal(new PluginDependencyGraph(List.of(broken)).order(PaperPluginDescriptor.Phase.SERVER), List.of(), "required defaults true");
        var optionalCycleA = paper("OptionalA", "dependencies:\n  server:\n    OptionalB:\n      load: BEFORE\n      required: false\n");
        var optionalCycleB = paper("OptionalB", "dependencies:\n  server:\n    OptionalA:\n      load: BEFORE\n      required: false\n");
        var downstream = paper("Downstream", "dependencies:\n  server:\n    OptionalA:\n      load: BEFORE\n      required: false\n");
        equal(new PluginDependencyGraph(List.of(optionalCycleA, optionalCycleB, downstream)).order(PaperPluginDescriptor.Phase.SERVER),
            List.of("optionalb", "optionala", "downstream"), "Paper cycle recovery retains downstream consumer");
        for (String version : List.of("garbage", "1.18", "1", "1.2.3.4", "1.x", "none")) {
            try {
                paperVersion(version);
                throw new AssertionError("invalid Paper API version accepted: " + version);
            } catch (org.bukkit.plugin.InvalidDescriptionException expected) {
                // Grammar/minimum are descriptor validation, independent of the host target.
            }
        }
        equal(paperVersion("99.99").getAPIVersion(), "99.99.0", "parser accepts future API versions");
        for (String invalid : List.of("load: BAD\n", "depend: wrong\n", "libraries: wrong\n")) {
            try {
                new org.bukkit.plugin.PluginDescriptionFile(new java.io.StringReader(
                    "name: Invalid\nversion: 1\nmain: fixture.Invalid\n" + invalid));
                throw new AssertionError("invalid descriptor accepted: " + invalid);
            } catch (org.bukkit.plugin.InvalidDescriptionException expected) {
                // Descriptor errors must surface before any class is initialized.
            }
        }
    }

    private static PaperPluginDescriptor paper(String name, String fields) throws Exception {
        return PaperPluginDescriptor.read(new java.io.ByteArrayInputStream(("name: " + name
            + "\nversion: 1\nmain: fixture.Main\napi-version: '26.2'\n" + fields).getBytes(StandardCharsets.UTF_8)));
    }

    private static PaperPluginDescriptor paperVersion(String version) throws Exception {
        return PaperPluginDescriptor.read(new java.io.ByteArrayInputStream(("name: Version\nversion: 1\nmain: fixture.Main\napi-version: '"
            + version + "'\n").getBytes(StandardCharsets.UTF_8)));
    }

    private static List<String> order() {
        return java.util.Arrays.stream(PluginHost.all()).map(org.bukkit.plugin.Plugin::getName).toList();
    }

    private static void equal(Object actual, Object expected, String message) {
        if (!java.util.Objects.equals(actual, expected)) {
            throw new AssertionError(message + ": expected " + expected + ", got " + actual);
        }
    }

    private static void fixture(Path directory, String name, boolean paper, String fields,
            String legacy) throws Exception {
        fixture(directory, name, paper, fields, legacy, "26.2");
    }

    private static void fixture(Path directory, String name, boolean paper, String fields,
            String legacy, String apiVersion) throws Exception {
        Files.createDirectories(directory);
        Path classes = Files.createDirectory(directory.resolve(name + "-classes"));
        Path source = classes.resolve(name + ".java");
        Files.writeString(source, "package fixture; public final class " + name
            + " extends org.bukkit.plugin.java.JavaPlugin { public " + name
            + "() { record(\"construct:\"); } public void onEnable() { record(\"enable:\"); }"
            + "private void record(String phase) { String key = \"foton.paper.order\";"
            + "System.setProperty(key, System.getProperty(key, \"\") + phase + getName() + \";\"); }}", StandardCharsets.UTF_8);
        int result = javax.tools.ToolProvider.getSystemJavaCompiler().run(null, null, null,
            "-classpath", System.getProperty("java.class.path"), "-d", classes.toString(), source.toString());
        equal(result, 0, "fixture compilation");
        try (JarOutputStream jar = new JarOutputStream(Files.newOutputStream(directory.resolve(name + ".jar")))) {
            entry(jar, "fixture/" + name + ".class", Files.readAllBytes(classes.resolve("fixture/" + name + ".class")));
            entry(jar, paper ? "paper-plugin.yml" : "plugin.yml", ("name: " + name
                + "\nversion: 1\nmain: fixture." + name + "\napi-version: '" + apiVersion + "'\n" + fields).getBytes(StandardCharsets.UTF_8));
            if (legacy != null) entry(jar, "plugin.yml", legacy.getBytes(StandardCharsets.UTF_8));
        }
    }

    private static void entry(JarOutputStream jar, String name, byte[] bytes) throws Exception {
        jar.putNextEntry(new JarEntry(name));
        jar.write(bytes);
        jar.closeEntry();
    }
}
