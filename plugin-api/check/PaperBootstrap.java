package foton;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.jar.JarEntry;
import java.util.jar.JarOutputStream;

/** Isolated plugin jars exercise bootstrap ownership before JavaPlugin exists. */
public final class PaperBootstrap {
    private PaperBootstrap() {}
    public static void main(String[] args) throws Exception { check(); }
    public static void check() throws Exception {
        LegacyLoggerAbiCheck.check();
        configuredHandlers();
        Path root = Files.createTempDirectory("foton-paper-bootstrap-");
        var serverField = org.bukkit.Bukkit.class.getDeclaredField("server");
        serverField.setAccessible(true);
        Object previous = serverField.get(null);
        org.bukkit.Server delegate = previous == null ? new FotonServer() : (org.bukkit.Server) previous;
        serverField.set(null, java.lang.reflect.Proxy.newProxyInstance(PaperBootstrap.class.getClassLoader(),
            new Class<?>[] {org.bukkit.Server.class}, (proxy, method, arguments) -> {
                if (method.getName().equals("getMinecraftVersion")) return "26.2";
                try { return method.invoke(delegate, arguments); }
                catch (java.lang.reflect.InvocationTargetException error) { throw error.getCause(); }
            }));
        try {
            Path late = root.resolve("late");
            fixture(late, "Late", "late", "");
            equal(PluginHost.loadAll(late.toString()), 1, "retained bootstrap context rejects late registration");
            PluginHost.disableAll();
            if (System.getProperty("foton.bootstrap.late-oracle-jar") != null) {
                Path external = root.resolve("late-external");
                Files.createDirectories(external);
                Files.copy(Path.of(System.getProperty("foton.bootstrap.late-oracle-jar")), external.resolve("probe.jar"));
                equal(PluginHost.loadAll(external.toString()), 1, "Paper-compiled late registration boundary");
                PluginHost.disableAll();
            }
            if (System.getProperty("foton.bootstrap.oracle-jar") != null) {
                Path external = root.resolve("external");
                Files.createDirectories(external);
                Files.copy(Path.of(System.getProperty("foton.bootstrap.oracle-jar")), external.resolve("probe.jar"));
                equal(PluginHost.loadAll(external.toString()), 1, "unchanged Paper-compiled bootstrap plugin");
                equal(CommandMap.dispatch(org.bukkit.Bukkit.getConsoleSender(), "bootstrap_probe"), true, "Paper-compiled command executes");
                PluginHost.disableAll();
            }
            Path good = root.resolve("good");
            fixture(good, "Good", "", "");
            System.setProperty("foton.bootstrap.trace", "");
            equal(PluginHost.loadAllOnLoad(good.toString()), 1, "bootstrap loads");
            equal(trace(), "bootstrap:Good;commands:Good;create:Good;construct:Good;load:Good;", "Paper phase order");
            equal(PluginHost.byName("Good").isEnabled(), false, "no early enable");
            equal(CommandMap.dispatch(org.bukkit.Bukkit.getConsoleSender(), "bootgood"), false, "no command invocation before enable");
            equal(PluginHost.enableAll(), 1, "enable loaded plugin");
            equal(trace(), "bootstrap:Good;commands:Good;create:Good;construct:Good;load:Good;enable:Good;", "enable follows load");
            equal(CommandMap.dispatch(org.bukkit.Bukkit.getConsoleSender(), "bootgood"), true, "bootstrap command works");
            PluginHost.disableAll();
            equal(CommandMap.dispatch(org.bukkit.Bukkit.getConsoleSender(), "bootgood"), false, "bootstrap command released");
            for (String failure : List.of("bootstrap", "create", "commands", "load", "registry", "missing")) {
                System.getProperties().remove("foton.bootstrap.context");
                Path broken = root.resolve(failure);
                fixture(broken, "Broken", failure, "permissions:\n  fixture.bootstrap: {}\n");
                System.setProperty("foton.bootstrap.trace", "");
                equal(PluginHost.loadAll(broken.toString()), 0, failure + " rejects plugin");
                equal(PluginHost.all().length, 0, failure + " leaves no plugin");
                equal(CommandMap.dispatch(org.bukkit.Bukkit.getConsoleSender(), "bootbroken"), false, failure + " leaves no command");
                equal(org.bukkit.Bukkit.getPluginManager().getPermission("fixture.bootstrap"), null, failure + " leaves no permission");
                equal(trace().contains("enable:"), false, failure + " never enables");
                Object retained = System.getProperties().remove("foton.bootstrap.context");
                if (retained instanceof io.papermc.paper.plugin.bootstrap.BootstrapContext context) {
                    registrationClosed(context.getLifecycleManager());
                }
                equal(EventBridge.handlerTypeCount(), 0, failure + " leaves no event listener");
                for (String field : List.of("pluginLoaders", "loadersByPlugin")) {
                    var loaders = PluginHost.class.getDeclaredField(field);
                    loaders.setAccessible(true);
                    equal(((java.util.Map<?, ?>) loaders.get(null)).size(), 0, failure + " leaves no loader ownership");
                }
                Object captured = System.getProperties().remove("foton.bootstrap.loader");
                if (captured instanceof ClassLoader loader) {
                    equal(loader.getResource("paper-plugin.yml"), null, failure + " closes plugin loader");
                }
            }
            Path defaults = root.resolve("default");
            fixture(defaults, "Default", "default", "");
            equal(PluginHost.loadAll(defaults.toString()), 1, "default createPlugin constructs no-arg main");
            PluginHost.disableAll();
            Path nullFallback = root.resolve("null-fallback");
            fixture(nullFallback, "NullFallback", "null", "");
            equal(PluginHost.loadAll(nullFallback.toString()), 1, "explicit null createPlugin uses ordinary main construction");
            PluginHost.disableAll();
            Path dependency = root.resolve("dependency");
            fixture(dependency, "Provider", "bootstrap", "");
            fixture(dependency, "Consumer", "", "dependencies:\n  server:\n    Provider:\n      load: BEFORE\n");
            System.setProperty("foton.bootstrap.trace", "");
            equal(PluginHost.loadAll(dependency.toString()), 0, "failed dependency rejects consumer");
            equal(trace().contains("load:Consumer"), false, "dependent never reaches onLoad");
            equal(PluginHost.all().length, 0, "dependent is unpublished");
            equal(CommandMap.dispatch(org.bukkit.Bukkit.getConsoleSender(), "bootconsumer"), false, "dependent commands released");
            Path pair = root.resolve("pair");
            fixture(pair, "First", "", ""); fixture(pair, "Second", "", "");
            System.setProperty("foton.bootstrap.trace", "");
            equal(PluginHost.loadAll(pair.toString()), 2, "both bootstrappers enabled");
            equal(trace().startsWith("bootstrap:First;bootstrap:Second;commands:First;commands:Second;create:First;"), true,
                "all bootstrap and command callbacks precede construction");
            PluginHost.disableAll();
            equal(trace().endsWith("disable:Second;disable:First;"), true, "reverse-order teardown");
        } finally {
            PluginHost.disableAll();
            serverField.set(null, previous);
            System.clearProperty("foton.bootstrap.trace");
            System.getProperties().remove("foton.bootstrap.loader");
            System.getProperties().remove("foton.bootstrap.context");
            try (var paths = Files.walk(root)) {
                for (Path path : paths.sorted(java.util.Comparator.reverseOrder()).toList()) Files.delete(path);
            }
        }
        System.out.println("Paper bootstrap checked: phase order, custom construction and failure rollback");
    }
    private static String trace() { return System.getProperty("foton.bootstrap.trace", ""); }
    private static void registrationClosed(io.papermc.paper.plugin.lifecycle.event.LifecycleEventManager events) {
        for (boolean configured : new boolean[] {false, true}) {
            try {
                if (configured) events.registerEventHandler(
                    io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents.COMMANDS.newHandler(event -> {}));
                else events.registerEventHandler(io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents.COMMANDS, event -> {});
                throw new AssertionError("failed bootstrap retained an open lifecycle manager");
            } catch (IllegalStateException expected) {
                equal(expected.getMessage(), "Cannot register lifecycle event handlers", "late registration diagnostic");
            }
        }
    }
    private static void configuredHandlers() {
        var events = new io.papermc.paper.plugin.lifecycle.event.FotonLifecycleEventManager();
        var order = new java.util.ArrayList<String>();
        var commands = io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents.COMMANDS;
        events.registerEventHandler(commands.newHandler(event -> order.add("last")).monitor());
        events.registerEventHandler(commands.newHandler(event -> order.add("later")).priority(8));
        events.registerEventHandler(commands.newHandler(event -> order.add("first")).monitor().priority(-1));
        var tags = io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents.TAGS.postFlatten(io.papermc.paper.registry.RegistryKey.ENCHANTMENT);
        events.registerEventHandler(tags.newHandler(event -> { throw new AssertionError("wrong lifecycle event dispatched"); }));
        io.papermc.paper.plugin.lifecycle.event.registrar.ReloadableRegistrarEvent<io.papermc.paper.command.brigadier.Commands> event = FotonCommands::new;
        events.dispatch(commands, event);
        equal(order, List.of("first", "later", "last"), "configured handlers preserve event identity and priority");
        var bootstrap = new io.papermc.paper.plugin.lifecycle.event.FotonLifecycleEventManager(true);
        try {
            bootstrap.registerEventHandler(tags.newHandler(ignored -> {}));
            throw new AssertionError("configured registry callback was silently accepted");
        } catch (UnsupportedOperationException expected) { }
        try {
            bootstrap.registerEventHandler(commands.newHandler(ignored -> {}).priority(1));
            throw new AssertionError("global bootstrap priority was silently accepted");
        } catch (UnsupportedOperationException expected) { }
    }
    private static void equal(Object actual, Object expected, String why) {
        if (!java.util.Objects.equals(actual, expected)) throw new AssertionError(why + ": " + actual + " != " + expected);
    }
    private static void fixture(Path directory, String name, String failure, String fields) throws Exception {
        Files.createDirectories(directory);
        Path sources = directory.resolve("src");
        Files.createDirectories(sources);
        String source = """
            package fixture;
            public final class Main extends org.bukkit.plugin.java.JavaPlugin {
              static void trace(String phase) { System.setProperty("foton.bootstrap.trace",
                System.getProperty("foton.bootstrap.trace", "") + phase + ":NAME;"); }
              static io.papermc.paper.plugin.bootstrap.BootstrapContext retained;
              static void late() {
                if (!"FAILURE".equals("late")) return;
                for (boolean configured : new boolean[] {false, true}) {
                  try {
                    if (configured) retained.getLifecycleManager().registerEventHandler(
                      io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents.COMMANDS.newHandler(event -> {}));
                    else retained.getLifecycleManager().registerEventHandler(
                      io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents.COMMANDS, event -> {});
                    throw new AssertionError("late registration accepted");
                  } catch (IllegalStateException expected) {
                    if (!"Cannot register lifecycle event handlers".equals(expected.getMessage())) throw expected;
                  }
                }
              }
              public Main(String value) { trace("construct"); }
              public Main() { trace("construct-default"); }
              public void onLoad() { late(); trace("load"); if ("FAILURE".equals("load")) throw new IllegalStateException("fixture load"); }
              public void onEnable() { trace("enable"); }
              public void onDisable() { trace("disable"); }
              public static final class Boot implements io.papermc.paper.plugin.bootstrap.PluginBootstrap {
                public void bootstrap(io.papermc.paper.plugin.bootstrap.BootstrapContext context) {
                  retained = context;
                  System.getProperties().put("foton.bootstrap.context", context);
                  System.getProperties().put("foton.bootstrap.loader", getClass().getClassLoader());
                  trace("bootstrap");
                  if ("FAILURE".equals("registry")) context.getLifecycleManager().registerEventHandler(
                    io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents.TAGS.postFlatten(io.papermc.paper.registry.RegistryKey.ENCHANTMENT), event -> {});
                  context.getLifecycleManager().registerEventHandler(io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents.COMMANDS, event -> {
                    late();
                    trace("commands");
                    ((io.papermc.paper.command.brigadier.Commands)event.registrar()).register(
                      io.papermc.paper.command.brigadier.Commands.literal("bootLABEL").executes(c -> 1).build());
                    if ("FAILURE".equals("commands")) throw new IllegalStateException("fixture commands");
                  });
                  if ("FAILURE".equals("bootstrap")) throw new IllegalStateException("fixture bootstrap");
                }
                public org.bukkit.plugin.java.JavaPlugin createPlugin(io.papermc.paper.plugin.bootstrap.PluginProviderContext context) {
                  late();
                  trace("create");
                  if ("FAILURE".equals("default")) return io.papermc.paper.plugin.bootstrap.PluginBootstrap.super.createPlugin(context);
                  if ("FAILURE".equals("null")) return null;
                  Main plugin = new Main("custom");
                  if ("FAILURE".equals("create")) throw new IllegalStateException("fixture create");
                  return plugin;
                }
              }
              public static final class Loader implements io.papermc.paper.plugin.loader.PluginLoader {
                public void classloader(io.papermc.paper.plugin.loader.PluginClasspathBuilder builder) {
                  builder.addLibrary(new io.papermc.paper.plugin.loader.library.impl.JarLibrary(
                    builder.getContext().getPluginSource().resolveSibling("missing.jar")));
                }
              }
            }
            """.replace("NAME", name).replace("LABEL", name.toLowerCase(java.util.Locale.ROOT)).replace("FAILURE", failure);
        Path sourceFile = sources.resolve("Main.java");
        Files.writeString(sourceFile, source);
        Path classes = directory.resolve("classes");
        Files.createDirectories(classes);
        int result = javax.tools.ToolProvider.getSystemJavaCompiler().run(null, null, null,
            "-nowarn", "-cp", System.getProperty("java.class.path"), "-d", classes.toString(), sourceFile.toString());
        equal(result, 0, "fixture compiles");
        try (var jar = new JarOutputStream(Files.newOutputStream(directory.resolve(name + ".jar")))) {
            try (var paths = Files.walk(classes)) {
                for (Path path : paths.filter(Files::isRegularFile).toList()) {
                    jar.putNextEntry(new JarEntry(classes.relativize(path).toString().replace('\\', '/')));
                    Files.copy(path, jar); jar.closeEntry();
                }
            }
            jar.putNextEntry(new JarEntry("paper-plugin.yml"));
            jar.write(("name: " + name + "\nversion: '1'\napi-version: '26.2'\nmain: fixture.Main\nbootstrapper: fixture.Main$Boot\n"
                + (failure.equals("missing") ? "loader: fixture.Main$Loader\n" : "") + fields).getBytes(java.nio.charset.StandardCharsets.UTF_8));
            jar.closeEntry();
        }
    }
}
