import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;
import org.bukkit.Bukkit;
import org.bukkit.plugin.IllegalPluginAccessException;
import org.bukkit.plugin.Plugin;
import org.bukkit.plugin.java.JavaPlugin;
import org.bukkit.plugin.java.PluginClassLoader;

/** Managed loading tasks must run, and their loader must survive retirement until they drain. */
public final class LoadingTasksCheck {
    private static final String NAME = "LoadingTasks";
    private static final String RESOURCE = "loading-task-resource.txt";
    private static CountDownLatch entered, releaseLoad, started, releaseTask, finished;
    private static final AtomicReference<Throwable> taskFailure = new AtomicReference<>();
    private static volatile Plugin owner;
    private static volatile PluginClassLoader loader;
    private static volatile org.bukkit.scheduler.BukkitTask task;
    private static boolean failLoad;

    private LoadingTasksCheck() {}

    public static void main(String[] args) throws Exception { check(); }

    static void check() throws Exception {
        Path root = Files.createTempDirectory("foton-loading-tasks-");
        try {
            Path successful = root.resolve("successful");
            pluginJar(successful, SuccessfulLoad.class);
            Checks.same(foton.PluginHost.loadAllOnLoad(successful.toString()), 1,
                "managed onLoad initialization must finish before enableAll");
            foton.PluginHost.cleanup(foton.PluginHost.byName(NAME));
            retirement(root.resolve("rollback"), true);
            retirement(root.resolve("retirement"), false);
        } finally {
            try (var paths = Files.walk(root)) {
                for (Path path : paths.sorted(java.util.Comparator.reverseOrder()).toList()) {
                    Files.deleteIfExists(path);
                }
            }
        }
    }

    private static void retirement(Path root, boolean failure) throws Exception {
        entered = new CountDownLatch(1);
        releaseLoad = new CountDownLatch(1);
        started = new CountDownLatch(1);
        releaseTask = new CountDownLatch(1);
        finished = new CountDownLatch(1);
        taskFailure.set(null);
        owner = null;
        loader = null;
        failLoad = failure;
        Path original = root.resolve("original");
        pluginJar(original, HeldLoad.class);
        AtomicReference<Throwable> loadFailure = new AtomicReference<>();
        Thread loading = new Thread(() -> {
            try {
                Checks.same(foton.PluginHost.loadAllOnLoad(original.toString()), 0,
                    "failed or retired loading plugin must not remain loaded");
            } catch (Throwable error) { loadFailure.set(error); }
        }, "managed-onload-async");
        loading.start();
        try {
            await(entered, "onLoad must observe its zero-delay async task starting");
            Checks.expect(!owner.isEnabled(), "async task must start before enable");
            Checks.same(activeInvocations(owner), 1, "loading task needs invocation accounting");
            if (!failure) {
                foton.PluginHost.cleanup(owner);
                rejectsScheduling(owner);
                Checks.expect(resourceAvailable(loader),
                    "retirement closed the loader while onLoad and its task were active");
            }
            releaseLoad.countDown();
            join(loading);
            Checks.expect(loadFailure.get() == null, "loading thread failed: " + loadFailure.get());
            Checks.expect(foton.PluginHost.byName(NAME) == null,
                "rollback/retirement must unpublish the old identity");
            Checks.expect(task.isCancelled(), "retired repeating task must stay cancelled");
            Checks.same(activeInvocations(owner), 1, "rollback lost the held invocation");
            Checks.expect(resourceAvailable(loader), "rollback closed a loader before task drain");
            rejectsScheduling(owner);

            Path replacement = root.resolve("replacement");
            pluginJar(replacement, Replacement.class);
            Checks.same(foton.PluginHost.loadAllOnLoad(replacement.toString()), 1,
                "a same-name replacement must load while the old task is draining");
            Plugin current = foton.PluginHost.byName(NAME);
            Checks.expect(current != owner && !current.isEnabled(),
                "replacement must have its own pre-enable identity");
            var teardown = asyncTeardownBarrier();
            Checks.expect(!teardown.isDone(), "teardown barrier passed a held invocation");
            releaseTask.countDown();
            await(finished, "old loading task did not finish");
            teardown.get(5, TimeUnit.SECONDS);
            Checks.expect(!resourceAvailable(loader), "retired loader did not close after task drain");
            Checks.expect(taskFailure.get() == null, "held task failed: " + taskFailure.get());
            Checks.same(foton.PluginHost.byName(NAME), current,
                "old teardown removed the replacement identity");
            CountDownLatch afterLoad = new CountDownLatch(1);
            Bukkit.getScheduler().runTaskAsynchronously(current, afterLoad::countDown);
            await(afterLoad, "open generation must run tasks between onLoad and enableAll");
        } finally {
            releaseLoad.countDown();
            releaseTask.countDown();
            join(loading);
            if (owner != null) foton.PluginHost.cleanup(owner);
            Plugin remaining = foton.PluginHost.byName(NAME);
            if (remaining != null) foton.PluginHost.cleanup(remaining);
        }
    }

    public static final class HeldLoad extends JavaPlugin {
        @Override public void onLoad() {
            owner = this;
            try { loader = managedLoader(this); }
            catch (Exception error) { throw new AssertionError(error); }
            task = getServer().getScheduler().runTaskTimerAsynchronously(this, () -> {
                started.countDown();
                try {
                    await(releaseTask, "loading task was not released");
                    Checks.expect(resourceAvailable(loader), "task lost its loader during rollback");
                    rejectsScheduling(this);
                    getServer().getScheduler().cancelTasks(this);
                } catch (Throwable error) { taskFailure.set(error); }
                finally { finished.countDown(); }
            }, 0, 1);
            await(started, "onLoad async task was accepted but dropped before enable");
            entered.countDown();
            await(releaseLoad, "onLoad was not released");
            if (failLoad) throw new IllegalStateException("intentional loading-task rollback");
        }
    }

    public static final class SuccessfulLoad extends JavaPlugin {
        @Override public void onLoad() {
            CountDownLatch once = new CountDownLatch(1);
            CountDownLatch repeats = new CountDownLatch(2);
            getServer().getScheduler().runTaskAsynchronously(this, once::countDown);
            org.bukkit.scheduler.BukkitTask repeating = getServer().getScheduler()
                .runTaskTimerAsynchronously(this, repeats::countDown, 0, 1);
            try {
                await(once, "one-shot onLoad initialization was dropped");
                await(repeats, "onLoad repeating initialization did not repeat");
            } finally { repeating.cancel(); }
        }
    }

    // The single async executor is deliberately held by the old task here.
    public static final class Replacement extends JavaPlugin { }

    private static void rejectsScheduling(Plugin plugin) {
        try {
            Bukkit.getScheduler().runTaskAsynchronously(plugin, () -> {});
            throw new AssertionError("retired loading generation accepted async work");
        } catch (IllegalPluginAccessException expected) { }
        try {
            Bukkit.getScheduler().runTask(plugin, () -> {});
            throw new AssertionError("retired loading generation accepted sync work");
        } catch (IllegalPluginAccessException expected) { }
    }

    private static Object hostField(String name) throws Exception {
        var field = foton.PluginHost.class.getDeclaredField(name);
        field.setAccessible(true);
        return field.get(null);
    }

    private static int activeInvocations(Plugin plugin) throws Exception {
        synchronized (hostField("lifecycle")) {
            Object state = ((java.util.Map<?, ?>) hostField("invocations")).get(plugin);
            Checks.expect(state != null, "active loading invocation was forgotten");
            var active = state.getClass().getDeclaredField("active");
            active.setAccessible(true);
            return active.getInt(state);
        }
    }

    private static PluginClassLoader managedLoader(Plugin plugin) throws Exception {
        synchronized (hostField("lifecycle")) {
            return (PluginClassLoader) ((java.util.Map<?, ?>) hostField("loadersByPlugin")).get(plugin);
        }
    }

    private static boolean resourceAvailable(PluginClassLoader source) throws Exception {
        try (InputStream stream = source.getResourceAsStream(RESOURCE)) { return stream != null; }
    }

    static java.util.concurrent.Future<?> asyncTeardownBarrier() throws Exception {
        var field = foton.FotonScheduler.class.getDeclaredField("OFF_TICK");
        field.setAccessible(true);
        var executor = (java.util.concurrent.ScheduledThreadPoolExecutor) field.get(null);
        Checks.same(executor.getCorePoolSize(), 1, "teardown barrier needs the single async worker");
        // A body latch precedes Invocation.close(); the next worker job follows deferred cleanup.
        return executor.submit(() -> {});
    }

    private static void await(CountDownLatch latch, String message) {
        try { Checks.expect(latch.await(5, TimeUnit.SECONDS), message); }
        catch (InterruptedException error) {
            Thread.currentThread().interrupt();
            throw new AssertionError(message, error);
        }
    }

    private static void join(Thread thread) throws InterruptedException {
        thread.join(TimeUnit.SECONDS.toMillis(5));
        Checks.expect(!thread.isAlive(), "loading thread did not finish");
    }

    private static void pluginJar(Path directory, Class<? extends JavaPlugin> main) throws Exception {
        Files.createDirectories(directory);
        try (var jar = new java.util.jar.JarOutputStream(Files.newOutputStream(directory.resolve(NAME + ".jar")))) {
            jar.putNextEntry(new java.util.jar.JarEntry("plugin.yml"));
            jar.write(("name: " + NAME + "\nversion: 1\nmain: " + main.getName() + "\n")
                .getBytes(java.nio.charset.StandardCharsets.UTF_8));
            jar.closeEntry();
            jar.putNextEntry(new java.util.jar.JarEntry(RESOURCE));
            jar.write(1);
            jar.closeEntry();
        }
    }
}
