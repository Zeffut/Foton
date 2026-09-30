import foton.CommandMap;
import foton.PluginHost;
import io.papermc.paper.command.brigadier.Commands;
import io.papermc.paper.plugin.bootstrap.BootstrapContext;
import io.papermc.paper.plugin.bootstrap.PluginBootstrap;
import io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;
import org.bukkit.Bukkit;
import org.bukkit.command.Command;
import org.bukkit.plugin.IllegalPluginAccessException;
import org.bukkit.plugin.java.JavaPlugin;
import org.bukkit.plugin.java.PluginClassLoader;

/** Both parent command phases must survive in one managed enable cycle. */
public final class LifecycleCommandsCheck {
    private static final String NAME = "LifecycleCommands";
    private enum Boundary { NONE, FAIL_ENABLE, DISABLE_ENABLE, DISCARD_ENABLE,
        FAIL_LATE, DISABLE_LATE, DISCARD_LATE }
    private static Boundary boundary;
    private static final List<String> steps = new ArrayList<>();
    private static final List<String> executions = new ArrayList<>();
    private static Commands registrar;
    private static ManagedPlugin owner;
    private static CountDownLatch started, release, finished;
    private static final AtomicReference<Throwable> taskFailure = new AtomicReference<>();

    private LifecycleCommandsCheck() {}

    public static void main(String[] args) throws Exception { check(); }

    static void check() throws Exception {
        Path root = Files.createTempDirectory("foton-lifecycle-commands-");
        try {
            for (Boundary selected : Boundary.values()) {
                boundary = selected;
                steps.clear();
                executions.clear();
                registrar = null;
                owner = null;
                started = new CountDownLatch(1);
                release = new CountDownLatch(1);
                finished = new CountDownLatch(1);
                taskFailure.set(null);
                try { exercise(root.resolve(selected.name())); }
                finally {
                    release.countDown();
                    if (owner != null) PluginHost.cleanup(owner);
                }
            }
        } finally {
            try (var paths = Files.walk(root)) {
                for (Path path : paths.sorted(java.util.Comparator.reverseOrder()).toList()) {
                    Files.deleteIfExists(path);
                }
            }
        }
    }

    private static void exercise(Path directory) throws Exception {
        pluginJar(directory);
        Checks.same(PluginHost.loadAllOnLoad(directory.toString()), 1,
            "managed command fixture must load");
        PluginClassLoader loader = managedLoader();
        int enabled = PluginHost.enableAll();
        if (boundary == Boundary.NONE) {
            Checks.same(enabled, 1, "command fixture must enable");
            Checks.same(steps, List.of("bootstrap:1", "constructor:1", "enable:1", "late1:1"),
                "early and onEnable handlers must run once in their respective phases");
            verifyCommands(1);
            Commands previous = registrar;
            PluginHost.disable(owner);
            noCommands();
            Checks.same(PluginHost.byName(NAME), owner, "disable must retain loaded identity");
            Checks.same(PluginHost.enableAll(), 1, "same managed identity must re-enable");
            Checks.same(steps, List.of("bootstrap:1", "constructor:1", "enable:1", "late1:1",
                "bootstrap:2", "constructor:2", "late1:2", "enable:2", "late2:2"),
                "each retained and new registration must run once per enable cycle");
            Checks.expect(registrar != previous, "re-enable must start a fresh command tree");
            verifyCommands(2);
            return;
        }
        Checks.same(enabled, 0, "failed or retired command fixture must not finish enabled");
        Checks.expect(!owner.isEnabled(), "retirement must disable the fixture");
        List<String> expected = switch (boundary) {
            case FAIL_LATE, DISABLE_LATE, DISCARD_LATE ->
                List.of("bootstrap:1", "constructor:1", "enable:1", "late1:1");
            default -> List.of("bootstrap:1", "constructor:1", "enable:1");
        };
        Checks.same(steps, expected, "late delivery crossed an enable failure/retirement boundary");
        noCommands();
        rejectsScheduling();
        if (boundary == Boundary.FAIL_ENABLE) {
            Checks.same(PluginHost.byName(NAME), owner, "failed enable must retain loaded identity");
            boundary = Boundary.NONE;
            Checks.same(PluginHost.enableAll(), 1, "failed enable must be retryable");
            Checks.same(steps, List.of("bootstrap:1", "constructor:1", "enable:1",
                "bootstrap:2", "constructor:2", "late1:2", "enable:2", "late2:2"),
                "failed enable must not retain a consumed cursor or lose pending handlers");
            verifyCommands(2);
        }
        if (boundary == Boundary.FAIL_LATE || boundary == Boundary.DISCARD_LATE) {
            Checks.same(activeInvocations(), 1, "late handler task lost invocation accounting");
            PluginHost.cleanup(owner);
            Checks.expect(PluginHost.byName(NAME) == null, "discard must unpublish identity");
            Checks.expect(resourceAvailable(loader), "discard closed loader before task drain");
            var teardown = LoadingTasksCheck.asyncTeardownBarrier();
            Checks.expect(!teardown.isDone(), "teardown barrier passed a held invocation");
            release.countDown();
            await(finished, "late handler task did not finish");
            teardown.get(5, TimeUnit.SECONDS);
            Checks.expect(!resourceAvailable(loader), "discard did not close drained loader");
            Checks.expect(taskFailure.get() == null, "late task failed: " + taskFailure.get());
        }
    }

    private static void verifyCommands(int cycle) {
        Checks.expect(CommandMap.get("b1early") == owner.earlyCommand,
            "late phase replaced the early command wrapper");
        for (String name : List.of("b1early", "b1constructor", "b1late1")) dispatch(name);
        if (cycle == 2) dispatch("b1late2");
        // Force the registrar fallback too: an early wrapper alone can hide a replaced registrar.
        CommandMap.unregister(owner.earlyCommand);
        dispatch("b1early");
        CommandMap.register(owner.earlyCommand);
    }

    private static void dispatch(String name) {
        executions.clear();
        Checks.expect(CommandMap.dispatch(Bukkit.getConsoleSender(), name), "command absent: " + name);
        Checks.same(executions, List.of(name), "command must execute exactly once: " + name);
    }

    private static void noCommands() {
        for (String name : List.of("b1early", "b1constructor", "b1late1", "b1late2")) {
            Checks.expect(CommandMap.get(name) == null, "retired command wrapper leaked: " + name);
            Checks.expect(!CommandMap.dispatch(Bukkit.getConsoleSender(), name),
                "retired registrar leaked: " + name);
        }
    }

    public static final class Bootstrap implements PluginBootstrap {
        @Override public void bootstrap(BootstrapContext context) {
            context.getLifecycleManager().registerEventHandler(LifecycleEvents.COMMANDS, event -> {
                registrar = (Commands) event.registrar();
                owner.cycle++;
                steps.add("bootstrap:" + owner.cycle);
                register(registrar, "b1early");
            });
        }
    }

    public static final class ManagedPlugin extends JavaPlugin {
        int cycle;
        Command earlyCommand;

        public ManagedPlugin() {
            owner = this;
            getLifecycleManager().registerEventHandler(LifecycleEvents.COMMANDS, event -> {
                Checks.expect(event.registrar() == registrar, "constructor needs bootstrap registrar");
                steps.add("constructor:" + cycle);
                register((Commands) event.registrar(), "b1constructor");
            });
        }

        @Override public void onEnable() {
            steps.add("enable:" + cycle);
            earlyCommand = CommandMap.get("b1early");
            dispatch("b1early");
            dispatch("b1constructor");
            int registration = cycle;
            getLifecycleManager().registerEventHandler(LifecycleEvents.COMMANDS, event -> {
                Checks.expect(event.registrar() == registrar, "late handler replaced early registrar");
                if (registration == cycle) {
                    try { Checks.same(activeInvocations(), 1, "late callback needs invocation accounting"); }
                    catch (Exception error) { throw new AssertionError(error); }
                }
                steps.add("late" + registration + ":" + cycle);
                register((Commands) event.registrar(), "b1late" + registration);
                if (boundary == Boundary.FAIL_LATE || boundary == Boundary.DISCARD_LATE) holdTask();
                if (boundary == Boundary.FAIL_LATE) throw new IllegalStateException("intentional late failure");
                if (boundary == Boundary.DISABLE_LATE || boundary == Boundary.DISCARD_LATE) retire();
            });
            if (boundary == Boundary.FAIL_ENABLE) throw new IllegalStateException("intentional enable failure");
            if (boundary == Boundary.DISABLE_ENABLE || boundary == Boundary.DISCARD_ENABLE) retire();
        }
    }

    private static void register(Commands commands, String name) {
        commands.register(Commands.literal(name).executes(context -> {
            executions.add(name);
            return 1;
        }).build());
    }

    private static void retire() {
        AtomicReference<Throwable> failure = new AtomicReference<>();
        Thread helper = new Thread(() -> {
            try {
                if (boundary == Boundary.DISCARD_ENABLE || boundary == Boundary.DISCARD_LATE) {
                    PluginHost.cleanup(owner);
                } else PluginHost.disable(owner);
                rejectsScheduling();
            } catch (Throwable error) { failure.set(error); }
        }, "lifecycle-command-retirement");
        helper.start();
        try { helper.join(TimeUnit.SECONDS.toMillis(5)); }
        catch (InterruptedException error) {
            Thread.currentThread().interrupt();
            throw new AssertionError(error);
        }
        Checks.expect(!helper.isAlive(), "command lifecycle callback held the operation lock");
        Checks.expect(failure.get() == null, "retirement failed: " + failure.get());
    }

    private static void holdTask() {
        Bukkit.getScheduler().runTaskAsynchronously(owner, () -> {
            started.countDown();
            try { await(release, "late task was not released"); }
            catch (Throwable error) { taskFailure.set(error); }
            finally { finished.countDown(); }
        });
        await(started, "late handler task did not enter");
    }

    private static void rejectsScheduling() {
        try {
            Bukkit.getScheduler().runTaskAsynchronously(owner, () -> {});
            throw new AssertionError("retired command lifecycle accepted new tasks");
        } catch (IllegalPluginAccessException expected) { }
    }

    private static Object hostField(String name) throws Exception {
        var field = PluginHost.class.getDeclaredField(name);
        field.setAccessible(true);
        return field.get(null);
    }

    private static PluginClassLoader managedLoader() throws Exception {
        synchronized (hostField("lifecycle")) {
            return (PluginClassLoader) ((java.util.Map<?, ?>) hostField("loadersByPlugin")).get(owner);
        }
    }

    private static int activeInvocations() throws Exception {
        synchronized (hostField("lifecycle")) {
            Object state = ((java.util.Map<?, ?>) hostField("invocations")).get(owner);
            Checks.expect(state != null, "active command lifecycle invocation was forgotten");
            var active = state.getClass().getDeclaredField("active");
            active.setAccessible(true);
            return active.getInt(state);
        }
    }

    private static boolean resourceAvailable(PluginClassLoader loader) throws Exception {
        try (var stream = loader.getResourceAsStream("paper-plugin.yml")) { return stream != null; }
    }

    private static void await(CountDownLatch latch, String message) {
        try { Checks.expect(latch.await(5, TimeUnit.SECONDS), message); }
        catch (InterruptedException error) {
            Thread.currentThread().interrupt();
            throw new AssertionError(message, error);
        }
    }

    private static void pluginJar(Path directory) throws Exception {
        Files.createDirectories(directory);
        try (var jar = new java.util.jar.JarOutputStream(Files.newOutputStream(directory.resolve(NAME + ".jar")))) {
            jar.putNextEntry(new java.util.jar.JarEntry("paper-plugin.yml"));
            jar.write(("name: " + NAME + "\nversion: 1\nmain: " + ManagedPlugin.class.getName()
                + "\nbootstrapper: " + Bootstrap.class.getName() + "\n")
                .getBytes(java.nio.charset.StandardCharsets.UTF_8));
            jar.closeEntry();
        }
    }
}
