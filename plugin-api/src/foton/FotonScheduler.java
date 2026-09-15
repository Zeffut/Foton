package foton;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;
import org.bukkit.plugin.Plugin;
import org.bukkit.scheduler.BukkitScheduler;
import org.bukkit.scheduler.BukkitTask;

/** Tasks a plugin asked to run, run where Bukkit promises they will.
 *
 * The promise is the point: `runTask` means "on the main thread, next tick",
 * and it is the only way a plugin can touch the world without racing it. So
 * nothing here runs a task when it is submitted. Foton drains this once per
 * tick, from the tick, and that is the only place a task body ever executes.
 *
 * Submission comes from any thread -- a plugin's own worker, a JVM thread, the
 * tick itself -- so the queue is concurrent. Draining is single-threaded by
 * construction, because there is one tick.
 */
public final class FotonScheduler implements BukkitScheduler {
    private static final int PENDING = 0;
    private static final int RUNNING = 1;
    private static final int CANCELLED = 2;
    private static final int CANCELLED_RUNNING = 3;
    private static final int FINISHED = 4;
    private static final ConcurrentLinkedQueue<Task> CLOSED = new ConcurrentLinkedQueue<>();
    private static final java.util.concurrent.ConcurrentHashMap<String, PluginTasks> pluginTasks =
        new java.util.concurrent.ConcurrentHashMap<>();
    private static final ConcurrentLinkedQueue<Scheduled> pending = new ConcurrentLinkedQueue<>();
    private static final java.util.concurrent.ConcurrentHashMap<Integer, Task> active =
        new java.util.concurrent.ConcurrentHashMap<>();
    private static final AtomicInteger nextId = new AtomicInteger(1);

    @Override
    public <T> java.util.concurrent.Future<T> callSyncMethod(
            Plugin plugin, java.util.concurrent.Callable<T> task) {
        java.util.concurrent.FutureTask<T> result = new java.util.concurrent.FutureTask<>(task);
        runTask(plugin, result);
        return result;
    }

    @Override
    public BukkitTask runTask(Plugin plugin, Runnable task) {
        return submit(plugin, task, 0, -1);
    }

    @Override
    public BukkitTask runTaskLater(Plugin plugin, Runnable task, long delayTicks) {
        return submit(plugin, task, Math.max(0, delayTicks), -1);
    }

    @Override
    public BukkitTask runTaskTimer(Plugin plugin, Runnable task, long delayTicks, long periodTicks) {
        return submit(plugin, task, Math.max(0, delayTicks), Math.max(1, periodTicks));
    }

    /** Where async tasks run. One thread: a plugin's background work is
     * almost always waiting on something, and a pool would mostly buy the
     * chance for two of a plugin's own tasks to race each other. */
    private static final java.util.concurrent.ScheduledThreadPoolExecutor OFF_TICK =
        offTickExecutor();

    private static java.util.concurrent.ScheduledThreadPoolExecutor offTickExecutor() {
        java.util.concurrent.ScheduledThreadPoolExecutor executor =
            new java.util.concurrent.ScheduledThreadPoolExecutor(1, runnable -> {
                Thread thread = new Thread(runnable, "foton-plugin-async");
                thread.setDaemon(true);
                return thread;
            });
        executor.setRemoveOnCancelPolicy(true);
        return executor;
    }

    @Override
    public BukkitTask runTaskAsynchronously(Plugin plugin, Runnable task) {
        return offTick(plugin, task, 0, -1);
    }

    @Override
    public BukkitTask runTaskLaterAsynchronously(Plugin plugin, Runnable task, long delayTicks) {
        return offTick(plugin, task, Math.max(0, delayTicks), -1);
    }

    @Override
    public BukkitTask runTaskTimerAsynchronously(
            Plugin plugin, Runnable task, long delayTicks, long periodTicks) {
        return offTick(plugin, task, Math.max(0, delayTicks), Math.max(1, periodTicks));
    }

    private static BukkitTask offTick(Plugin plugin, Runnable body, long delay, long period) {
        PluginTasks owner = requireAccepting(plugin);
        Async task = new Async(
            nextId.getAndIncrement(), plugin, owner, period > 0);
        if (!owner.register(task)) throw rejected(plugin);
        active.put(task.id, task);
        Runnable guarded = () -> {
            if (!task.tryStart()) return;
            if (!owner.accepting()) {
                task.cancel();
                task.finishRun();
                return;
            }
            task.thread = Thread.currentThread();
            try {
                body.run();
            } catch (Throwable error) {
                // Nothing above this catches: an exception on the executor
                // thread would silently stop a repeating task forever.
                System.out.println("[scheduler] " + plugin.getName()
                    + " threw in an async task: " + error);
            } finally {
                task.thread = null;
                task.finishRun();
            }
        };
        try {
            // A tick is fifty milliseconds. Bukkit measures async delays in ticks
            // too, which reads oddly and is what plugins pass.
            long delayMillis = delay * 50;
            java.util.concurrent.ScheduledFuture<?> handle = period > 0
                ? OFF_TICK.scheduleAtFixedRate(guarded, delayMillis, period * 50,
                    java.util.concurrent.TimeUnit.MILLISECONDS)
                : OFF_TICK.schedule(guarded, delayMillis,
                    java.util.concurrent.TimeUnit.MILLISECONDS);
            task.bind(handle);
        } catch (RuntimeException error) {
            task.cancel();
            throw error;
        }
        if (!owner.accepting() || task.isCancelled()) {
            task.cancel();
            throw rejected(plugin);
        }
        return task;
    }

    /** Makes scheduling legal before onLoad, where Bukkit permits registration. */
    static void activatePlugin(Plugin plugin) {
        pluginTasks.put(key(plugin), new PluginTasks());
    }

    /** Closes and drains the scheduling side of cleanup before callbacks run. */
    static void beginPluginCleanup(Plugin plugin) {
        PluginTasks tasks = pluginTasks.get(key(plugin));
        if (tasks == null) return;
        tasks.close();
        pluginTasks.remove(key(plugin), tasks);
    }

    private static PluginTasks requireAccepting(Plugin plugin) {
        PluginTasks tasks = pluginTasks.get(key(plugin));
        if (tasks == null || !tasks.accepting()) throw rejected(plugin);
        return tasks;
    }

    private static IllegalStateException rejected(Plugin plugin) {
        return new IllegalStateException(
            "Plugin " + plugin.getName() + " is no longer accepting scheduled tasks");
    }

    private static String key(Plugin plugin) {
        return plugin.getName().toLowerCase(java.util.Locale.ROOT);
    }

    /** A task running off the tick, cancellable from anywhere. */
    private static final class Async extends Task {
        final boolean repeating;
        volatile java.util.concurrent.ScheduledFuture<?> handle;
        volatile Thread thread;

        Async(int id, Plugin plugin, PluginTasks owner, boolean repeating) {
            super(id, plugin, owner);
            this.repeating = repeating;
        }

        void bind(java.util.concurrent.ScheduledFuture<?> handle) {
            this.handle = handle;
            if (isCancelled()) handle.cancel(false);
        }

        @Override public boolean isSync() { return false; }

        @Override public void cancel() {
            boolean running = cancelTransition();
            java.util.concurrent.ScheduledFuture<?> future = handle;
            if (future != null) future.cancel(false);
            if (!running) release();
        }

        void finishRun() {
            if (repeating && owner.accepting()
                    && phase.compareAndSet(RUNNING, PENDING)) return;
            phase.set(FINISHED);
            release();
        }
    }

    @Override
    public int scheduleSyncDelayedTask(Plugin plugin, Runnable task, long delayTicks) {
        return runTaskLater(plugin, task, delayTicks).getTaskId();
    }

    @Override
    public int scheduleSyncDelayedTask(Plugin plugin, Runnable task) {
        return runTask(plugin, task).getTaskId();
    }

    @Override
    public int scheduleSyncRepeatingTask(
            Plugin plugin, Runnable task, long delayTicks, long periodTicks) {
        return runTaskTimer(plugin, task, delayTicks, periodTicks).getTaskId();
    }

    @Override
    public void cancelTask(int taskId) {
        BukkitTask task = active.get(taskId);
        if (task != null) task.cancel();
    }

    @Override
    public void cancelTasks(Plugin plugin) {
        PluginTasks tasks = pluginTasks.get(key(plugin));
        if (tasks != null) tasks.cancelCurrent();
    }

    @Override
    public boolean isCurrentlyRunning(int taskId) {
        Task task = active.get(taskId);
        return task != null && task.isRunning();
    }

    @Override
    public java.util.List<org.bukkit.scheduler.BukkitWorker> getActiveWorkers() {
        java.util.List<org.bukkit.scheduler.BukkitWorker> workers = new java.util.ArrayList<>();
        for (Task task : active.values()) {
            if (task.isRunning()) workers.add(new Worker(task));
        }
        return java.util.List.copyOf(workers);
    }

    @Override
    public java.util.List<BukkitTask> getPendingTasks() {
        return java.util.List.copyOf(active.values());
    }

    private static Scheduled submit(Plugin plugin, Runnable body, long delay, long period) {
        PluginTasks owner = requireAccepting(plugin);
        Scheduled task = new Scheduled(
            nextId.getAndIncrement(), plugin, owner, body, delay, period);
        if (!owner.register(task)) throw rejected(plugin);
        active.put(task.id, task);
        if (!task.publish()) {
            throw rejected(plugin);
        }
        return task;
    }

    /** Runs what this tick owes. Called by Foton, from the tick, once.
     *
     * Returns how many task bodies ran, which is what a diagnostic wants and
     * what the test asserts on.
     */
    public static int tick() {
        List<Scheduled> due = new ArrayList<>();
        List<Scheduled> keep = new ArrayList<>();
        Scheduled task;
        while ((task = pending.poll()) != null) {
            if (!task.isPending()) continue;
            // Count this tick down first, then ask whether the task is due.
            // Doing it the other way round costs a tick on every repeat: a
            // period of 2 would fire every 3. CraftBukkit stores an absolute
            // "next run" tick instead, and this is the same arithmetic said
            // as a countdown -- delay 0 and delay 1 both mean the next tick.
            task.remaining--;
            if (task.remaining > 0) {
                keep.add(task);
                continue;
            }
            due.add(task);
        }
        for (Scheduled waiting : keep) waiting.publish();

        int ran = 0;
        for (Scheduled ready : due) {
            if (!ready.tryStart()) continue;
            if (!ready.owner.accepting()) {
                ready.cancel();
                ready.finishRun();
                continue;
            }
            try {
                ready.thread = Thread.currentThread();
                ready.body.run();
                ran++;
            } catch (Throwable error) {
                // A plugin's task throwing must not stop the tick, and must not
                // reach Foton: an exception crossing JNI is a crash.
                System.out.println("[scheduler] " + ready.plugin.getName()
                    + " threw in a task: " + error);
            } finally {
                ready.thread = null;
                ready.finishRun();
            }
        }
        return ran;
    }

    /** Forgets everything, for a shutdown. */
    public static void clear() {
        for (PluginTasks tasks : List.copyOf(pluginTasks.values())) tasks.close();
        pluginTasks.clear();
        for (BukkitTask task : List.copyOf(active.values())) task.cancel();
        active.clear();
        pending.clear();
        OFF_TICK.purge();
    }

    static int trackedTaskCount(String pluginName) {
        int count = 0;
        for (BukkitTask task : active.values()) {
            if (task.getOwner().getName().equals(pluginName)) count++;
        }
        return count;
    }

    static int queuedTaskCount(String pluginName) {
        int count = 0;
        for (Scheduled task : pending) {
            if (task.plugin.getName().equals(pluginName)) count++;
        }
        return count;
    }

    static boolean acceptsTasks(String pluginName) {
        PluginTasks tasks = pluginTasks.get(pluginName.toLowerCase(java.util.Locale.ROOT));
        return tasks != null && tasks.accepting();
    }

    /** One plugin generation's lock-free task registry.
     *
     * <p>Registration publishes into the current queue and rechecks its
     * identity. Cleanup atomically replaces that queue with {@link #CLOSED}.
     * A racing task is therefore either in cleanup's captured queue or sees
     * the tombstone and removes itself after publication. */
    private static final class PluginTasks {
        private final AtomicReference<ConcurrentLinkedQueue<Task>> tasks =
            new AtomicReference<>(new ConcurrentLinkedQueue<>());

        boolean accepting() {
            return tasks.get() != CLOSED;
        }

        boolean register(Task task) {
            ConcurrentLinkedQueue<Task> registry = tasks.get();
            if (registry == CLOSED) return false;
            task.registry = registry;
            registry.add(task);
            if (tasks.get() == registry) return true;
            task.cancel();
            return false;
        }

        void cancelCurrent() {
            ConcurrentLinkedQueue<Task> registry = tasks.get();
            if (registry == CLOSED) return;
            cancelSnapshot(registry);
        }

        void close() {
            ConcurrentLinkedQueue<Task> registry = tasks.getAndSet(CLOSED);
            if (registry == CLOSED) return;
            cancelSnapshot(registry);
        }

        void remove(Task task) {
            ConcurrentLinkedQueue<Task> registry = task.registry;
            if (registry == null) return;
            registry.remove(task);
            task.registry = null;
        }

        private static void cancelSnapshot(ConcurrentLinkedQueue<Task> registry) {
            for (Object entry : registry.toArray()) ((Task) entry).cancel();
        }
    }

    private abstract static class Task implements BukkitTask {
        final int id;
        final Plugin plugin;
        final PluginTasks owner;
        final AtomicInteger phase = new AtomicInteger(PENDING);
        volatile ConcurrentLinkedQueue<Task> registry;

        Task(int id, Plugin plugin, PluginTasks owner) {
            this.id = id;
            this.plugin = plugin;
            this.owner = owner;
        }

        @Override public int getTaskId() { return id; }
        @Override public Plugin getOwner() { return plugin; }
        @Override public boolean isCancelled() {
            int state = phase.get();
            return state == CANCELLED || state == CANCELLED_RUNNING;
        }

        boolean isPending() {
            return phase.get() == PENDING;
        }

        boolean isRunning() {
            int state = phase.get();
            return state == RUNNING || state == CANCELLED_RUNNING;
        }

        boolean tryStart() {
            return phase.compareAndSet(PENDING, RUNNING);
        }

        /** Returns whether a body already won the right to finish running. */
        boolean cancelTransition() {
            while (true) {
                int state = phase.get();
                if (state == CANCELLED_RUNNING) return true;
                if (state == CANCELLED || state == FINISHED) return false;
                int cancelled = state == RUNNING ? CANCELLED_RUNNING : CANCELLED;
                if (phase.compareAndSet(state, cancelled)) return state == RUNNING;
            }
        }

        void release() {
            active.remove(id, this);
            owner.remove(this);
        }
    }

    private static final class Worker implements org.bukkit.scheduler.BukkitWorker {
        private final BukkitTask task;
        Worker(BukkitTask task) { this.task = task; }
        @Override public int getTaskId() { return task.getTaskId(); }
        @Override public Plugin getOwner() { return task.getOwner(); }
        @Override public Thread getThread() {
            if (task instanceof Async async) return async.thread;
            if (task instanceof Scheduled scheduled) return scheduled.thread;
            return null;
        }
    }

    private static final class Scheduled extends Task {
        final Runnable body;
        final long period;
        long remaining;
        volatile Thread thread;

        Scheduled(
                int id, Plugin plugin, PluginTasks owner, Runnable body,
                long delay, long period) {
            super(id, plugin, owner);
            this.body = body;
            this.period = period;
            this.remaining = delay;
        }

        @Override public boolean isSync() { return true; }
        @Override public void cancel() {
            boolean running = cancelTransition();
            pending.remove(this);
            if (!running) release();
        }

        boolean publish() {
            if (!isPending() || !owner.accepting()) {
                cancel();
                return false;
            }
            pending.add(this);
            if (isPending() && owner.accepting()) return true;
            cancel();
            return false;
        }

        void finishRun() {
            if (period > 0 && owner.accepting()
                    && phase.compareAndSet(RUNNING, PENDING)) {
                remaining = period;
                publish();
                return;
            }
            phase.set(FINISHED);
            release();
        }
    }
}
