package foton;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.Comparator;
import java.util.HexFormat;
import java.util.jar.JarEntry;
import java.util.jar.JarOutputStream;

/** Two-plugin bootstrap fixtures shared by the in-process check and pinned Paper runs. */
public final class PaperBootstrapGraph {
    private PaperBootstrapGraph() {}

    public static void main(String[] args) throws Exception {
        if (args.length == 2 && args[0].equals("--emit")) {
            Path output = Path.of(args[1]);
            createCases(output);
            writeOracleScenario(output);
            return;
        }
        check();
    }

    public static void check() throws Exception {
        Path root = Files.createTempDirectory("foton-bootstrap-graph-");
        var serverField = org.bukkit.Bukkit.class.getDeclaredField("server");
        serverField.setAccessible(true);
        Object previous = serverField.get(null);
        org.bukkit.Server delegate = previous == null ? new FotonServer() : (org.bukkit.Server) previous;
        serverField.set(null, java.lang.reflect.Proxy.newProxyInstance(PaperBootstrapGraph.class.getClassLoader(),
            new Class<?>[] {org.bukkit.Server.class}, (proxy, method, arguments) -> {
                if (method.getName().equals("getMinecraftVersion")) return "26.2";
                try { return method.invoke(delegate, arguments); }
                catch (java.lang.reflect.InvocationTargetException error) { throw error.getCause(); }
            }));
        try {
            createCases(root);
            verifyCases(root);
            String oracle = System.getProperty("foton.bootstrap.graph.oracleRoot");
            if (oracle != null) verifyCases(Path.of(oracle));
        } finally {
            PluginHost.disableAll();
            serverField.set(null, previous);
            System.clearProperty("foton.bootstrap.graph.trace");
            System.getProperties().remove("foton.bootstrap.graph.failedLoader");
            System.getProperties().remove("foton.bootstrap.graph.consumerLoader");
            try (var paths = Files.walk(root)) {
                for (Path path : paths.sorted(Comparator.reverseOrder()).toList()) Files.delete(path);
            }
        }
        System.out.println("Paper bootstrap graph checked: visibility, missing/failing dependencies, independent phases");
    }

    private static void verifyCases(Path root) {
        verify(root.resolve("required-joined"), 2,
                "bootstrap:ZuluProvider;visible:AlphaConsumer:true;bootstrap:AlphaConsumer;",
                "construct:AlphaConsumer;construct:ZuluProvider;", 2);
        verify(root.resolve("required-isolated"), 2,
                "bootstrap:ZuluProvider;visible:AlphaConsumer:true;bootstrap:AlphaConsumer;",
                "construct:AlphaConsumer;construct:ZuluProvider;", 2);
        verify(root.resolve("independent"), 2,
                "visible:AlphaConsumer:false;bootstrap:AlphaConsumer;bootstrap:ZuluProvider;",
                "construct:AlphaConsumer;construct:ZuluProvider;", 2);
        verify(root.resolve("optional-missing"), 1,
                "visible:AlphaConsumer:false;bootstrap:AlphaConsumer;",
                "construct:AlphaConsumer;", 1);
        verify(root.resolve("required-missing"), 0, "", "", 0);
        verify(root.resolve("failed-provider"), 1,
                "bootstrap:FailProvider;visible:AlphaConsumer:true;bootstrap:AlphaConsumer;",
                "construct:AlphaConsumer;", 1);
        verify(root.resolve("no-bootstrapper"), 1, "", "construct:NoBootstrapConsumer;", 1);
        verify(root.resolve("server-missing-bootstrap"), 0,
                "visible:AlphaConsumer:false;bootstrap:AlphaConsumer;", "", 0);
        verify(root.resolve("bootstrap-plain-provider"), 1, "", "construct:PlainProvider;", 1);
    }

    private static void verify(Path plugins, int loaded, String bootstrap, String construction,
            int enabled) {
        System.setProperty("foton.bootstrap.graph.trace", "");
        equal(PluginHost.loadAllOnLoad(plugins.toString()), loaded, plugins.getFileName() + " load count");
        String trace = System.getProperty("foton.bootstrap.graph.trace");
        if (bootstrap.isEmpty()) {
            equal(trace.contains("bootstrap:") || trace.contains("visible:"), false,
                plugins.getFileName() + " must not bootstrap");
        } else {
            equal(trace.startsWith(bootstrap), true, plugins.getFileName() + " bootstrap order");
        }
        if (construction.isEmpty()) {
            equal(trace.contains("construct:") || trace.contains("load:") || trace.contains("enable:"),
                false, plugins.getFileName() + " must not reach SERVER lifecycle");
        } else {
            equal(trace.contains(construction), true, plugins.getFileName() + " server order");
        }
        equal(PluginHost.all().length, loaded, plugins.getFileName() + " published plugins");
        for (var plugin : PluginHost.all()) {
            equal(plugin.isEnabled(), false, plugins.getFileName() + " has no early enable");
        }
        if (plugins.getFileName().toString().equals("failed-provider")) {
            equal(trace.contains("construct:FailProvider"), false,
                "failed bootstrap provider must not construct");
        }
        if (plugins.getFileName().toString().equals("bootstrap-plain-provider")) {
            equal(trace.contains("AlphaConsumer"), false,
                "missing bootstrap provider has no consumer lifecycle");
        }
        Object consumerLoader = System.getProperties().remove("foton.bootstrap.graph.consumerLoader");
        if (plugins.getFileName().toString().equals("server-missing-bootstrap")) {
            equal(consumerLoader instanceof ClassLoader, true, "server-rejected bootstrap loader captured");
            equal(((ClassLoader) consumerLoader).getResource("paper-plugin.yml"), null,
                "server-rejected bootstrap loader closed after discovery");
        }
        if (plugins.getFileName().toString().equals("failed-provider")) {
            Object failed = System.getProperties().remove("foton.bootstrap.graph.failedLoader");
            equal(failed instanceof ClassLoader, true, "failed provider loader captured");
            equal(((ClassLoader) failed).getResource("paper-plugin.yml"), null,
                "failed provider loader closed after bootstrap phase");
        }
        noRetainedBootstrapLoaders();
        equal(PluginHost.enableAll(), enabled, plugins.getFileName() + " enable count");
        PluginHost.disableAll();
    }

    private static void noRetainedBootstrapLoaders() {
        try {
            var loadersField = PluginHost.class.getDeclaredField("loadersByPlugin");
            loadersField.setAccessible(true);
            var parentField = org.bukkit.plugin.java.PluginClassLoader.class.getDeclaredField("dependencies");
            parentField.setAccessible(true);
            for (Object loader : ((java.util.Map<?, ?>) loadersField.get(null)).values()) {
                Object parent = parentField.get(loader);
                if (!parent.getClass().getName().endsWith("PluginHost$DependencyClassLoader")) continue;
                var bootstrapField = parent.getClass().getDeclaredField("bootstrapLoaders");
                bootstrapField.setAccessible(true);
                equal(((java.util.Map<?, ?>) bootstrapField.get(parent)).isEmpty(), true,
                    "surviving loader retains no bootstrap sibling loaders");
            }
        } catch (ReflectiveOperationException error) {
            throw new AssertionError("cannot inspect bootstrap loader ownership", error);
        }
    }

    private static void equal(Object actual, Object expected, String why) {
        if (!java.util.Objects.equals(actual, expected)) {
            throw new AssertionError(why + ": " + actual + " != " + expected);
        }
    }

    private static void createCases(Path root) throws Exception {
        Path compiled = root.resolve("compiled");
        Files.createDirectories(compiled);
        Path sources = root.resolve("sources");
        Files.createDirectories(sources);
        Path provider = sources.resolve("Provider.java");
        Path consumer = sources.resolve("Consumer.java");
        Files.writeString(provider, """
            package fixturegraph;
            public final class Provider extends org.bukkit.plugin.java.JavaPlugin {
              static void trace(String value) { System.out.println("BOOTSTRAP_GRAPH " + value);
                System.out.println("ORACLE:bootstrap_graph.phase=" + value);
                if (value.startsWith("enable:")) System.out.println("ORACLE:lifecycle.enabled=" + value.substring(7));
                System.setProperty("foton.bootstrap.graph.trace", System.getProperty("foton.bootstrap.graph.trace", "") + value + ";"); }
              public Provider() { trace("construct:" + getDescription().getName()); }
              public void onLoad() { trace("load:" + getName()); }
              public void onEnable() { trace("enable:" + getName()); }
              public void onDisable() { trace("disable:" + getName()); }
              public static final class Marker {}
              public static final class Boot implements io.papermc.paper.plugin.bootstrap.PluginBootstrap {
                public void bootstrap(io.papermc.paper.plugin.bootstrap.BootstrapContext context) {
                  String name = context.getConfiguration().getName();
                  trace("bootstrap:" + name);
                  if (name.equals("FailProvider")) {
                    System.getProperties().put("foton.bootstrap.graph.failedLoader", getClass().getClassLoader());
                    throw new IllegalStateException("provider fixture failure");
                  }
                }
              }
            }
            """);
        Files.writeString(consumer, """
            package fixturegraph;
            public final class Consumer extends org.bukkit.plugin.java.JavaPlugin {
              static void trace(String value) { System.out.println("BOOTSTRAP_GRAPH " + value);
                System.out.println("ORACLE:bootstrap_graph.phase=" + value);
                if (value.startsWith("enable:")) System.out.println("ORACLE:lifecycle.enabled=" + value.substring(7));
                System.setProperty("foton.bootstrap.graph.trace", System.getProperty("foton.bootstrap.graph.trace", "") + value + ";"); }
              public Consumer() { trace("construct:" + getDescription().getName()); }
              public void onLoad() { trace("load:" + getName()); }
              public void onEnable() { trace("enable:" + getName()); }
              public void onDisable() { trace("disable:" + getName()); }
              public static final class Boot implements io.papermc.paper.plugin.bootstrap.PluginBootstrap {
                public void bootstrap(io.papermc.paper.plugin.bootstrap.BootstrapContext context) {
                  String name = context.getConfiguration().getName();
                  System.getProperties().put("foton.bootstrap.graph.consumerLoader", getClass().getClassLoader());
                  try {
                    Class.forName("fixturegraph.Provider$Marker", false, getClass().getClassLoader());
                    trace("visible:" + name + ":true");
                  } catch (ClassNotFoundException missing) { trace("visible:" + name + ":false"); }
                  trace("bootstrap:" + name);
                }
              }
            }
            """);
        String classpath = System.getProperty("foton.bootstrap.graph.compileCp", System.getProperty("java.class.path"));
        int compiledOk = javax.tools.ToolProvider.getSystemJavaCompiler().run(null, null, null,
            "-nowarn", "-cp", classpath, "-d", compiled.toString(), provider.toString(), consumer.toString());
        equal(compiledOk, 0, "bootstrap graph fixtures compile");
        String joined = "dependencies:\n  bootstrap:\n    ZuluProvider:\n      load: BEFORE\n"
            + "  server:\n    ZuluProvider:\n      load: AFTER\n";
        String isolated = "dependencies:\n  bootstrap:\n    ZuluProvider:\n      load: BEFORE\n      join-classpath: false\n"
            + "  server:\n    ZuluProvider:\n      load: AFTER\n";
        caseJar(root, compiled, "required-joined", "ZuluProvider", true, "");
        caseJar(root, compiled, "required-joined", "AlphaConsumer", false, joined);
        caseJar(root, compiled, "required-isolated", "ZuluProvider", true, "");
        caseJar(root, compiled, "required-isolated", "AlphaConsumer", false, isolated);
        caseJar(root, compiled, "independent", "ZuluProvider", true, "");
        caseJar(root, compiled, "independent", "AlphaConsumer", false, "");
        caseJar(root, compiled, "optional-missing", "AlphaConsumer", false,
            "dependencies:\n  bootstrap:\n    MissingProvider:\n      required: false\n");
        caseJar(root, compiled, "required-missing", "AlphaConsumer", false,
            "dependencies:\n  bootstrap:\n    MissingProvider: {}\n");
        caseJar(root, compiled, "failed-provider", "FailProvider", true, "");
        caseJar(root, compiled, "failed-provider", "AlphaConsumer", false,
            "dependencies:\n  bootstrap:\n    FailProvider:\n      load: BEFORE\n");
        caseJar(root, compiled, "no-bootstrapper", "NoBootstrapConsumer", false,
            "dependencies:\n  bootstrap:\n    MissingProvider: {}\n");
        caseJar(root, compiled, "server-missing-bootstrap", "AlphaConsumer", false,
            "dependencies:\n  server:\n    MissingProvider: {}\n");
        caseJar(root, compiled, "bootstrap-plain-provider", "PlainProvider", true, "");
        caseJar(root, compiled, "bootstrap-plain-provider", "AlphaConsumer", false,
            "dependencies:\n  bootstrap:\n    PlainProvider:\n      load: BEFORE\n");
    }

    private static void writeOracleScenario(Path root) throws Exception {
        Path provider = root.resolve("required-joined/ZuluProvider.jar").toAbsolutePath();
        Path consumer = root.resolve("required-joined/AlphaConsumer.jar").toAbsolutePath();
        String scenario = """
            {
              "plugin": "AlphaConsumer",
              "server_version": "26.2",
              "setup": {"seed": 8675309, "jars": [
                {"path": "%s", "sha256": "%s"},
                {"path": "%s", "sha256": "%s"}
              ]},
              "actions": [{"type": "status_ping"}],
              "observations": {"startup_order": ["AlphaConsumer", "ZuluProvider"],
                "required_markers": ["bootstrap_graph.phase", "lifecycle.enabled"]},
              "normalizers": [{"path": "/server/port", "kind": "port"}]
            }
            """.formatted(json(provider.toString()), sha256(provider),
                json(consumer.toString()), sha256(consumer));
        Files.writeString(root.resolve("oracle-required-joined.json"), scenario);
    }

    private static String json(String value) {
        return value.replace("\\", "\\\\").replace("\"", "\\\"");
    }

    private static String sha256(Path file) throws Exception {
        return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256")
            .digest(Files.readAllBytes(file)));
    }

    private static void caseJar(Path root, Path compiled, String scenario, String name,
            boolean provider, String fields) throws Exception {
        Path output = root.resolve(scenario);
        Files.createDirectories(output);
        String main = provider ? "Provider" : "Consumer";
        try (var jar = new JarOutputStream(Files.newOutputStream(output.resolve(name + ".jar")))) {
            try (var paths = Files.walk(compiled)) {
                for (Path path : paths.filter(Files::isRegularFile)
                        .filter(path -> path.getFileName().toString().startsWith(main + ".")
                            || path.getFileName().toString().startsWith(main + "$")).toList()) {
                    jar.putNextEntry(new JarEntry(compiled.relativize(path).toString().replace('\\', '/')));
                    Files.copy(path, jar);
                    jar.closeEntry();
                }
            }
            jar.putNextEntry(new JarEntry("paper-plugin.yml"));
            String bootstrapper = name.equals("NoBootstrapConsumer") || name.equals("PlainProvider") ? ""
                : "bootstrapper: fixturegraph." + main + "$Boot\n";
            jar.write(("name: " + name + "\nversion: '1'\napi-version: '26.2'\nmain: fixturegraph."
                + main + "\n" + bootstrapper + fields)
                .getBytes(StandardCharsets.UTF_8));
            jar.closeEntry();
        }
    }
}
