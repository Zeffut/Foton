package foton;

/** Test-only access to package-private lifecycle retention diagnostics. */
public final class LifecycleDiagnostics {
    private LifecycleDiagnostics() {}

    public static int schedulerTasks(String plugin) {
        return FotonScheduler.trackedTaskCount(plugin);
    }

    public static int queuedTasks(String plugin) {
        return FotonScheduler.queuedTaskCount(plugin);
    }

    public static boolean removeQueuedTask(org.bukkit.scheduler.BukkitTask task) {
        return pending().remove(task);
    }

    /** Replays the operation tick uses after holding a task in its local keep list. */
    public static void republishQueuedTask(org.bukkit.scheduler.BukkitTask task) {
        try {
            java.lang.reflect.Method publish = task.getClass().getDeclaredMethod("publish");
            publish.setAccessible(true);
            publish.invoke(task);
        } catch (ReflectiveOperationException error) {
            throw new AssertionError("cannot republish a tick-local task", error);
        }
    }

    private static java.util.Queue<?> pending() {
        try {
            java.lang.reflect.Field field = FotonScheduler.class.getDeclaredField("pending");
            field.setAccessible(true);
            return (java.util.Queue<?>) field.get(null);
        } catch (ReflectiveOperationException error) {
            throw new AssertionError("cannot inspect the scheduler pending queue", error);
        }
    }

    /** Exercises scheduler APIs while the old lifecycle monitor is contended. */
    public static boolean gameTickApisBlockOnLifecycleMonitor(
            org.bukkit.plugin.Plugin plugin) {
        final Object monitor;
        try {
            java.lang.reflect.Field field =
                FotonScheduler.class.getDeclaredField("lifecycleLock");
            field.setAccessible(true);
            monitor = field.get(null);
        } catch (NoSuchFieldException removed) {
            return false;
        } catch (ReflectiveOperationException error) {
            throw new AssertionError("cannot inspect the scheduler lifecycle monitor", error);
        }

        org.bukkit.scheduler.BukkitTask cancellable =
            org.bukkit.Bukkit.getScheduler().runTaskLater(plugin, () -> {}, 100);
        boolean blocked = blocksOn(monitor,
            () -> org.bukkit.Bukkit.getScheduler().runTaskLater(plugin, () -> {}, 100));
        blocked |= blocksOn(monitor,
            () -> org.bukkit.Bukkit.getScheduler().runTaskLaterAsynchronously(
                plugin, () -> {}, 100));
        blocked |= blocksOn(monitor, cancellable::cancel);
        return blocked;
    }

    private static boolean blocksOn(Object monitor, Runnable operation) {
        java.util.concurrent.CountDownLatch held = new java.util.concurrent.CountDownLatch(1);
        java.util.concurrent.CountDownLatch release = new java.util.concurrent.CountDownLatch(1);
        java.util.concurrent.CountDownLatch started = new java.util.concurrent.CountDownLatch(1);
        java.util.concurrent.CountDownLatch completed = new java.util.concurrent.CountDownLatch(1);
        java.util.concurrent.atomic.AtomicReference<Throwable> failure =
            new java.util.concurrent.atomic.AtomicReference<>();
        Thread holder = new Thread(() -> {
            synchronized (monitor) {
                held.countDown();
                await(release, "lifecycle monitor holder was not released");
            }
        }, "fixture-lifecycle-monitor");
        Thread caller = new Thread(() -> {
            started.countDown();
            try {
                operation.run();
            } catch (Throwable error) {
                failure.set(error);
            } finally {
                completed.countDown();
            }
        }, "fixture-game-tick-api");
        holder.start();
        await(held, "lifecycle monitor was not acquired");
        caller.start();
        await(started, "scheduler operation did not start");
        boolean blocked;
        try {
            blocked = awaitBlockedOrCompleted(caller, completed);
        } finally {
            release.countDown();
            join(holder);
            join(caller);
        }
        if (failure.get() != null) {
            throw new AssertionError("scheduler operation failed", failure.get());
        }
        return blocked;
    }

    private static boolean awaitBlockedOrCompleted(
            Thread caller, java.util.concurrent.CountDownLatch completed) {
        long deadline = System.nanoTime() + java.util.concurrent.TimeUnit.SECONDS.toNanos(5);
        while (System.nanoTime() < deadline) {
            if (caller.getState() == Thread.State.BLOCKED) return true;
            if (completed.getCount() == 0) return false;
            Thread.onSpinWait();
        }
        throw new AssertionError("scheduler operation neither completed nor blocked");
    }

    private static void await(java.util.concurrent.CountDownLatch latch, String message) {
        try {
            if (!latch.await(5, java.util.concurrent.TimeUnit.SECONDS)) {
                throw new AssertionError(message);
            }
        } catch (InterruptedException interrupted) {
            Thread.currentThread().interrupt();
            throw new AssertionError(message, interrupted);
        }
    }

    private static void join(Thread thread) {
        try {
            thread.join(5000);
            if (thread.isAlive()) throw new AssertionError(thread.getName() + " did not finish");
        } catch (InterruptedException interrupted) {
            Thread.currentThread().interrupt();
            throw new AssertionError(thread.getName() + " join was interrupted", interrupted);
        }
    }

    public static boolean schedulerAccepts(String plugin) {
        return FotonScheduler.acceptsTasks(plugin);
    }

    public static int schedulerGenerations(String plugin) {
        return FotonScheduler.generationCount(plugin);
    }

    public static int hostReferences(String plugin) {
        return PluginHost.lifecycleReferenceCount(plugin);
    }

    public static int eventTypes(String event) {
        return EventBridge.eventTypeCount(event);
    }
}
