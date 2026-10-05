package foton;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.jar.JarEntry;
import java.util.jar.JarOutputStream;

/** Real plugin loaders paused between registration and SERVER graph connection. */
public final class DependencyReadiness {
    private static volatile Runnable preparationProbe;
    private static volatile Runnable constructorProbe;

    private DependencyReadiness() {}

    public static void pausePreparation() { preparationProbe.run(); }
    public static void checkConstructor() { constructorProbe.run(); }

    static void check(Path root) throws Exception {
        readiness(root.resolve("bukkit-direct"), false, null);
        readiness(root.resolve("paper-direct"), true, null);
        readiness(root.resolve("paper-after"), true, "AFTER");
        readiness(root.resolve("paper-omit"), true, "OMIT");
        System.out.println("Dependency readiness checked: late Bukkit/Paper, transitive AFTER/OMIT, retry and construction");
    }

    private static void readiness(Path root, boolean paper, String transitiveOrder) throws Exception {
        Path classes = compile(root, transitiveOrder != null);
        Path live = root.resolve("live");
        pluginJar(live, classes, "ReadyD", false, "");
        pluginJar(live, classes, "ReadyConsumer", paper, paper
            ? "dependencies:\n  server:\n    ReadyService:\n      required: false\n"
            : "softdepend: [ReadyService]\n");
        equal(PluginHost.loadAll(live.toString()), 2, "D and optional consumer are already live");
        ClassLoader consumer = PluginHost.byName("ReadyConsumer").getClass().getClassLoader();
        ClassLoader dependency = PluginHost.byName("ReadyD").getClass().getClassLoader();
        Path batch = root.resolve("batch");
        String providerDependencies = transitiveOrder == null
            ? "depend: [ReadyD]\n"
            : "dependencies:\n  server:\n    ReadyMiddle:\n      load: " + transitiveOrder + "\n";
        pluginJar(batch, classes, "AReadyProvider", transitiveOrder != null,
            "provides: [ReadyService]\n" + providerDependencies);
        if (transitiveOrder != null) {
            pluginJar(batch, classes, "ReadyMiddle", false, "depend: [ReadyD]\n");
        }
        // Lexically last, with no ordering edges: its real custom-loader callback
        // holds the preparation loop after the provider(s) have registered.
        pluginJar(batch, classes, "ZReadyPause", true, "loader: fixture.ZReadyPause$Loader\n");

        CountDownLatch entered = new CountDownLatch(1);
        CountDownLatch release = new CountDownLatch(1);
        AtomicInteger constructors = new AtomicInteger();
        preparationProbe = () -> {
            entered.countDown();
            await(release, "release custom-loader callback");
        };
        constructorProbe = () -> {
            // Readiness must precede constructors, not merely onLoad or enable.
            try {
                Class<?> api = Class.forName("fixture.AReadyProvider$Api", false, consumer);
                Class<?> base = transitiveOrder == null ? api.getSuperclass() : api.getSuperclass().getSuperclass();
                equal(base.getClassLoader(), dependency, "constructor sees the connected transitive graph");
                constructors.incrementAndGet();
            } catch (ClassNotFoundException error) {
                throw new AssertionError("SERVER graph must be ready before construction", error);
            }
        };
        var workers = Executors.newFixedThreadPool(2, task -> {
            Thread thread = new Thread(task, "dependency-readiness-check");
            thread.setDaemon(true);
            return thread;
        });
        try {
            var loading = workers.submit(() -> PluginHost.loadAll(batch.toString()));
            await(entered, "enter later preparation callback");
            // A bounded future detects a lifecycle-lock hold or a blocking readiness
            // wait: the callback cannot finish until this lookup has returned.
            var lookup = workers.submit(() -> {
                Object reserved = reservedProvider();
                if (reserved == null) throw new AssertionError("registration must reserve the first generation before lookup");
                equal(PluginHost.byName("AReadyProvider"), null, "provider not yet constructed");
                try {
                    Class.forName("fixture.AReadyProvider$Api", false, consumer);
                    throw new AssertionError("unconnected provider must not be exposed");
                } catch (ClassNotFoundException expected) {
                    // Retryable absence before entering the provider's defineClass.
                } catch (LinkageError error) {
                    throw new AssertionError("partial SERVER graph entered provider class definition", error);
                }
                return reserved;
            });
            Object reserved = lookup.get(10, TimeUnit.SECONDS);
            equal(constructors.get(), 0, "constructor waits for preparation to finish");
            release.countDown();
            equal(loading.get(20, TimeUnit.SECONDS), transitiveOrder == null ? 2 : 3, "prepared batch loads");
            equal(reservedProvider(), reserved, "readiness preserves the reserved generation");
            equal(constructors.get(), 1, "provider constructor resolved through the ready graph");
            Class<?> api = Class.forName("fixture.AReadyProvider$Api", true, consumer);
            equal(api.getClassLoader(), PluginHost.byName("AReadyProvider").getClass().getClassLoader(),
                "fresh lookup uses the first provider loader");
            Class<?> base = api.getSuperclass();
            if (transitiveOrder != null) {
                equal(base.getClassLoader(), PluginHost.byName("ReadyMiddle").getClass().getClassLoader(),
                    "same-batch transitive class comes from its owning loader");
                base = base.getSuperclass();
                equal(PluginHost.all()[2].getName(), "AReadyProvider", "AFTER/OMIT provider constructed before middle");
            }
            equal(base.getClassLoader(), dependency, "D-only superclass keeps its live identity");
            equal(api.getMethod("value").invoke(api.getDeclaredConstructor().newInstance()), 42,
                "early absence did not poison later class definition or initialization");
        } finally {
            release.countDown();
            workers.shutdown();
            if (!workers.awaitTermination(30, TimeUnit.SECONDS)) {
                throw new AssertionError("readiness workers failed to finish after release");
            }
            preparationProbe = null;
            constructorProbe = null;
            PluginHost.disableAll();
        }
    }

    private static Object reservedProvider() throws Exception {
        synchronized (PluginHost.lifecycleLock()) {
            var states = PluginHost.class.getDeclaredField("statesByPlugin");
            states.setAccessible(true);
            Object consumer = ((Map<?, ?>) states.get(null)).get(PluginHost.byName("ReadyConsumer"));
            var providers = consumer.getClass().getDeclaredField("providers");
            providers.setAccessible(true);
            var reference = (java.lang.ref.Reference<?>) ((Map<?, ?>) providers.get(consumer)).get("readyservice");
            return reference == null ? null : reference.get();
        }
    }

    private static void await(CountDownLatch latch, String message) {
        try {
            if (!latch.await(20, TimeUnit.SECONDS)) throw new AssertionError("timeout: " + message);
        } catch (InterruptedException error) {
            Thread.currentThread().interrupt();
            throw new AssertionError(message, error);
        }
    }

    private static Path compile(Path root, boolean transitive) throws Exception {
        Path classes = Files.createDirectories(root.resolve("classes"));
        Map<String, String> bodies = Map.of(
            "ReadyD", "public static class Base { public int value() { return 42; } }",
            "ReadyConsumer", "",
            "AReadyProvider", "public AReadyProvider() { foton.DependencyReadiness.checkConstructor(); }"
                + "public static class Api extends " + (transitive ? "ReadyMiddle.Api" : "ReadyD.Base") + " {}",
            "ReadyMiddle", "public static class Api extends ReadyD.Base {}",
            "ZReadyPause", "public static class Loader implements io.papermc.paper.plugin.loader.PluginLoader {"
                + "public void classloader(io.papermc.paper.plugin.loader.PluginClasspathBuilder builder) {"
                + "foton.DependencyReadiness.pausePreparation(); }}");
        List<String> args = new ArrayList<>(List.of("-classpath", System.getProperty("java.class.path"),
            "-d", classes.toString()));
        for (var entry : bodies.entrySet()) {
            Path source = classes.resolve(entry.getKey() + ".java");
            Files.writeString(source, "package fixture; public class " + entry.getKey()
                + " extends org.bukkit.plugin.java.JavaPlugin {" + entry.getValue() + "}", StandardCharsets.UTF_8);
            args.add(source.toString());
        }
        equal(javax.tools.ToolProvider.getSystemJavaCompiler().run(null, null, null, args.toArray(new String[0])),
            0, "readiness fixture compilation");
        return classes;
    }

    private static void pluginJar(Path directory, Path classes, String name, boolean paper, String fields)
            throws Exception {
        Files.createDirectories(directory);
        try (JarOutputStream jar = new JarOutputStream(Files.newOutputStream(directory.resolve(name + ".jar")));
                var files = Files.list(classes.resolve("fixture"))) {
            for (Path file : files.filter(path -> path.getFileName().toString().equals(name + ".class")
                    || path.getFileName().toString().startsWith(name + "$")).toList()) {
                jar.putNextEntry(new JarEntry("fixture/" + file.getFileName()));
                jar.write(Files.readAllBytes(file));
                jar.closeEntry();
            }
            jar.putNextEntry(new JarEntry(paper ? "paper-plugin.yml" : "plugin.yml"));
            jar.write(("name: " + name + "\nversion: 1\nmain: fixture." + name
                + "\napi-version: '26.2'\n" + fields).getBytes(StandardCharsets.UTF_8));
            jar.closeEntry();
        }
    }

    private static void equal(Object actual, Object expected, String message) {
        if (!java.util.Objects.equals(actual, expected)) {
            throw new AssertionError(message + ": expected " + expected + ", got " + actual);
        }
    }
}
