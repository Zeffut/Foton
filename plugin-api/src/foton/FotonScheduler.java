package foton;

import java.lang.invoke.MethodHandles;
import java.lang.invoke.VarHandle;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.concurrent.atomic.AtomicInteger;
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
    private static final ConcurrentLinkedQueue<Scheduled> pending = new ConcurrentLinkedQueue<>();
    private static final java.util.concurrent.ConcurrentHashMap<Integer, Task> active =
        new java.util.concurrent.ConcurrentHashMap<>();
    private static final AtomicInteger nextId = new AtomicInteger(1);
    private static final VarHandle GENERATIONS;
    private static volatile GenerationTable generations = GenerationTable.EMPTY;

    static {
        try {
            GENERATIONS = MethodHandles.lookup().findStaticVarHandle(
                FotonScheduler.class, "generations", GenerationTable.class);
        } catch (ReflectiveOperationException error) {
            throw new ExceptionInInitializerError(error);
        }
    }

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

    /** CraftScheduler's check: a disabled plugin -- one inside its own
     * `onDisable`, most often -- is refused rather than given a task nothing
     * will ever cancel. Plugins catch this to do the work inline instead. */
    static void validate(Plugin plugin) {
        if (plugin == null) {
            throw new IllegalArgumentException("Plugin cannot be null");
        }
        PluginGeneration generation = generationTable().find(plugin);
        if (generation == null || !generation.accepting()) {
            throw new org.bukkit.plugin.IllegalPluginAccessException(
                "Plugin attempted to register a task while disabled: " + plugin.getName());
        }
    }

    private static BukkitTask offTick(Plugin plugin, Runnable body, long delay, long period) {
        java.util.Objects.requireNonNull(body, "task");
        PluginGeneration generation = requireAccepting(plugin);
        Async task = new Async(nextId.getAndIncrement(), plugin, generation, period > 0);
        active.put(task.id, task);
        if (!generation.accepting()) {
            task.cancel();
            throw rejected(plugin);
        }
        Runnable guarded = () -> {
            if (!task.tryStart()) return;
            if (!generation.accepting()) {
                task.cancel();
                task.finishRun();
                return;
            }
            PluginHost.Invocation invocation = PluginHost.beginTaskInvocation(generation);
            if (invocation == null) {
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
                plugin.getLogger().log(java.util.logging.Level.WARNING,
                    "Plugin " + plugin.getName() + " generated an exception in task " + task.id, error);
            } finally {
                task.thread = null;
                task.finishRun();
                invocation.close();
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
        if (!generation.accepting() || task.isCancelled()) {
            task.cancel();
            throw rejected(plugin);
        }
        return task;
    }

    /** Makes scheduling legal before onLoad, where Bukkit permits registration. */
    static void activatePlugin(Plugin plugin) {
        PluginGeneration added = new PluginGeneration(plugin);
        while (true) {
            GenerationTable current = generationTable();
            if (current.find(plugin) != null) return;
            GenerationTable updated = current.with(added);
            if (GENERATIONS.compareAndSet(current, updated)) return;
        }
    }

    /** Closes the exact plugin generation before callbacks release its resources.
     *
     * <p>The generation tombstone is a release-store. A submission either
     * publishes to {@link #active} before cleanup scans it, or observes the
     * tombstone in its post-publication recheck and removes itself. Scheduling
     * and sync cancellation therefore take no lifecycle lock. */
    /** Rejects publication/start while leaving existing resources visible to onDisable. */
    static void stopPluginSubmissions(Plugin plugin) {
        PluginGeneration generation = generationTable().find(plugin);
        if (generation != null) generation.close();
    }

    static void beginPluginCleanup(Plugin plugin) {
        PluginGeneration generation = generationTable().find(plugin);
        if (generation == null) return;
        generation.close();
        cancelGeneration(generation);
        removeGeneration(generation);
    }

    private static void removeGeneration(PluginGeneration generation) {
        while (true) {
            GenerationTable current = generationTable();
            if (!current.contains(generation)) return;
            if (GENERATIONS.compareAndSet(current, current.without(generation))) return;
        }
    }

    private static PluginGeneration requireAccepting(Plugin plugin) {
        validate(plugin);
        PluginGeneration generation = generationTable().find(plugin);
        if (generation == null || !generation.accepting()) throw rejected(plugin);
        return generation;
    }

    private static GenerationTable generationTable() {
        return (GenerationTable) GENERATIONS.getAcquire();
    }

    private static org.bukkit.plugin.IllegalPluginAccessException rejected(Plugin plugin) {
        return new org.bukkit.plugin.IllegalPluginAccessException(
            "Plugin " + plugin.getName() + " is no longer accepting scheduled tasks");
    }

    /** A task running off the tick, cancellable from anywhere. */
    private static final class Async extends Task {
        final boolean repeating;
        volatile java.util.concurrent.ScheduledFuture<?> handle;
        volatile Thread thread;

        Async(int id, Plugin plugin, PluginGeneration generation, boolean repeating) {
            super(id, plugin, generation);
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
            if (repeating && generation.accepting()
                    && comparePhase(RUNNING, PENDING)) return;
            finishTerminal();
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
        PluginGeneration generation = generationTable().find(plugin);
        if (generation != null) cancelGeneration(generation);
    }

    private static void cancelGeneration(PluginGeneration generation) {
        for (Task task : active.values()) {
            if (task.generation == generation) task.cancel();
        }
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
        java.util.Objects.requireNonNull(body, "task");
        PluginGeneration generation = requireAccepting(plugin);
        Scheduled task = new Scheduled(
            nextId.getAndIncrement(), plugin, generation, body, delay, period);
        active.put(task.id, task);
        if (!task.publish()) throw rejected(plugin);
        return task;
    }

    /** Runs what this tick owes. Called by Foton, from the tick, once.
     *
     * Returns how many task bodies ran, which is what a diagnostic wants and
     * what the test asserts on.
     */
    public static int tick() {
        FotonChunkRequests.tick();
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
            if (!ready.generation.accepting()) {
                ready.cancel();
                ready.finishRun();
                continue;
            }
            PluginHost.Invocation invocation = PluginHost.beginTaskInvocation(ready.generation);
            if (invocation == null) {
                ready.cancel();
                ready.finishRun();
                continue;
            }
            {
                ready.thread = Thread.currentThread();
            }
            try {
                ready.body.run();
                ran++;
            } catch (Throwable error) {
                // A plugin's task throwing must not stop the tick, and must not
                // reach Foton: an exception crossing JNI is a crash.
                // CraftScheduler's wording, with the stack, as above.
                ready.plugin.getLogger().log(java.util.logging.Level.WARNING, String.format(
                    "Task #%s for %s generated an exception",
                    ready.id, ready.plugin.getDescription().getFullName()), error);
            } finally {
                ready.thread = null;
                ready.finishRun();
                invocation.close();
            }
        }
        return ran;
    }

    /** Forgets everything, for a shutdown. */
    public static void clear() {
        while (true) {
            GenerationTable current = generationTable();
            current.closeAll();
            if (GENERATIONS.compareAndSet(current, GenerationTable.EMPTY)) break;
        }
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
        for (PluginGeneration generation : generationTable().slots) {
            if (generation != null && generation.plugin.getName().equalsIgnoreCase(pluginName)
                    && generation.accepting()) return true;
        }
        return false;
    }

    static int generationCount(String pluginName) {
        int count = 0;
        for (PluginGeneration generation : generationTable().slots) {
            if (generation != null
                    && generation.plugin.getName().equalsIgnoreCase(pluginName)) count++;
        }
        return count;
    }

    /** One plugin instance's lifecycle generation. */
    static final class PluginGeneration {
        private static final int OPEN = 0;
        private static final int CLOSED = 1;
        private static final VarHandle STATE;

        static {
            try {
                STATE = MethodHandles.lookup().findVarHandle(
                    PluginGeneration.class, "state", int.class);
            } catch (ReflectiveOperationException error) {
                throw new ExceptionInInitializerError(error);
            }
        }

        final Plugin plugin;
        private volatile int state = OPEN;

        PluginGeneration(Plugin plugin) {
            this.plugin = plugin;
        }

        boolean accepting() {
            return (int) STATE.getAcquire(this) == OPEN;
        }

        void close() {
            STATE.setRelease(this, CLOSED);
        }
    }

    /** Immutable identity-keyed table, rebuilt only at lifecycle boundaries. */
    private static final class GenerationTable {
        static final GenerationTable EMPTY = new GenerationTable(new PluginGeneration[0], 0);
        final PluginGeneration[] slots;
        final int size;

        GenerationTable(PluginGeneration[] slots, int size) {
            this.slots = slots;
            this.size = size;
        }

        PluginGeneration find(Plugin plugin) {
            if (slots.length == 0) return null;
            int index = identityHash(plugin) & (slots.length - 1);
            while (true) {
                PluginGeneration generation = slots[index];
                if (generation == null) return null;
                if (generation.plugin == plugin) return generation;
                index = (index + 1) & (slots.length - 1);
            }
        }

        boolean contains(PluginGeneration expected) {
            return find(expected.plugin) == expected;
        }

        GenerationTable with(PluginGeneration added) {
            PluginGeneration[] entries = new PluginGeneration[size + 1];
            int index = copyEntries(entries, null);
            entries[index] = added;
            return build(entries);
        }

        GenerationTable without(PluginGeneration removed) {
            if (!contains(removed)) return this;
            if (size == 1) return EMPTY;
            PluginGeneration[] entries = new PluginGeneration[size - 1];
            copyEntries(entries, removed);
            return build(entries);
        }

        void closeAll() {
            for (PluginGeneration generation : slots) {
                if (generation != null) generation.close();
            }
        }

        private int copyEntries(
                PluginGeneration[] destination, PluginGeneration excluded) {
            int index = 0;
            for (PluginGeneration generation : slots) {
                if (generation != null && generation != excluded) {
                    destination[index++] = generation;
                }
            }
            return index;
        }

        private static GenerationTable build(PluginGeneration[] entries) {
            int capacity = 4;
            while (capacity < entries.length * 2) capacity *= 2;
            PluginGeneration[] slots = new PluginGeneration[capacity];
            for (PluginGeneration generation : entries) {
                int index = identityHash(generation.plugin) & (capacity - 1);
                while (slots[index] != null) index = (index + 1) & (capacity - 1);
                slots[index] = generation;
            }
            return new GenerationTable(slots, entries.length);
        }

        private static int identityHash(Plugin plugin) {
            int hash = System.identityHashCode(plugin);
            return hash ^ (hash >>> 16);
        }
    }

    private abstract static class Task implements BukkitTask {
        private static final VarHandle PHASE;

        static {
            try {
                PHASE = MethodHandles.lookup().findVarHandle(Task.class, "phase", int.class);
            } catch (ReflectiveOperationException error) {
                throw new ExceptionInInitializerError(error);
            }
        }

        final int id;
        final Plugin plugin;
        final PluginGeneration generation;
        private volatile int phase = PENDING;

        Task(int id, Plugin plugin, PluginGeneration generation) {
            this.id = id;
            this.plugin = plugin;
            this.generation = generation;
        }

        @Override public int getTaskId() { return id; }
        @Override public Plugin getOwner() { return plugin; }
        @Override public boolean isCancelled() {
            int state = phase();
            return state == CANCELLED || state == CANCELLED_RUNNING;
        }

        boolean isPending() {
            return phase() == PENDING;
        }

        boolean isRunning() {
            int state = phase();
            return state == RUNNING || state == CANCELLED_RUNNING;
        }

        boolean tryStart() {
            return comparePhase(PENDING, RUNNING);
        }

        int phase() {
            return (int) PHASE.getVolatile(this);
        }

        boolean comparePhase(int expected, int update) {
            return PHASE.compareAndSet(this, expected, update);
        }

        /** Returns whether a body already won the right to finish running. */
        boolean cancelTransition() {
            while (true) {
                int state = phase();
                if (state == CANCELLED_RUNNING) return true;
                if (state == CANCELLED || state == FINISHED) return false;
                int cancelled = state == RUNNING ? CANCELLED_RUNNING : CANCELLED;
                if (comparePhase(state, cancelled)) return state == RUNNING;
            }
        }

        void release() {
            active.remove(id, this);
        }

        void finishTerminal() {
            while (true) {
                int state = phase();
                if (state == CANCELLED || state == FINISHED) return;
                int finished = state == CANCELLED_RUNNING ? CANCELLED : FINISHED;
                if (comparePhase(state, finished)) {
                    release();
                    return;
                }
            }
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
                int id, Plugin plugin, PluginGeneration generation, Runnable body,
                long delay, long period) {
            super(id, plugin, generation);
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
            if (!isPending() || !generation.accepting()) {
                cancel();
                return false;
            }
            pending.add(this);
            if (isPending() && generation.accepting()) return true;
            cancel();
            return false;
        }

        void finishRun() {
            if (period > 0 && generation.accepting()
                    && comparePhase(RUNNING, PENDING)) {
                remaining = period;
                publish();
                return;
            }
            finishTerminal();
        }
    }
}
