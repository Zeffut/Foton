package foton;

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
    private static final ConcurrentLinkedQueue<Scheduled> pending = new ConcurrentLinkedQueue<>();
    private static final AtomicInteger nextId = new AtomicInteger(1);

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

    @Override
    public void cancelTask(int taskId) {
        for (Scheduled task : pending) {
            if (task.id == taskId) {
                task.cancelled = true;
            }
        }
    }

    @Override
    public void cancelTasks(Plugin plugin) {
        for (Scheduled task : pending) {
            if (task.plugin == plugin) {
                task.cancelled = true;
            }
        }
    }

    private static Scheduled submit(Plugin plugin, Runnable body, long delay, long period) {
        Scheduled task = new Scheduled(nextId.getAndIncrement(), plugin, body, delay, period);
        pending.add(task);
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
            if (task.cancelled) {
                continue;
            }
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
            if (task.period > 0) {
                task.remaining = task.period;
                keep.add(task);
            }
        }
        pending.addAll(keep);

        int ran = 0;
        for (Scheduled ready : due) {
            try {
                ready.body.run();
                ran++;
            } catch (Throwable error) {
                // A plugin's task throwing must not stop the tick, and must not
                // reach Foton: an exception crossing JNI is a crash.
                System.out.println("[scheduler] " + ready.plugin.getName()
                    + " threw in a task: " + error);
            }
        }
        return ran;
    }

    /** Forgets everything, for a shutdown. */
    public static void clear() {
        pending.clear();
    }

    private static final class Scheduled implements BukkitTask {
        final int id;
        final Plugin plugin;
        final Runnable body;
        final long period;
        long remaining;
        volatile boolean cancelled;

        Scheduled(int id, Plugin plugin, Runnable body, long delay, long period) {
            this.id = id;
            this.plugin = plugin;
            this.body = body;
            this.period = period;
            this.remaining = delay;
        }

        @Override public int getTaskId() { return id; }
        @Override public void cancel() { cancelled = true; }
    }
}
