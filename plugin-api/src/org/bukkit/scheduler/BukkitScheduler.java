package org.bukkit.scheduler;

import org.bukkit.plugin.Plugin;
import java.util.concurrent.Callable;
import java.util.concurrent.Future;

public interface BukkitScheduler {
    <T> Future<T> callSyncMethod(Plugin plugin, Callable<T> task);
    BukkitTask runTask(Plugin plugin, Runnable task);
    BukkitTask runTaskLater(Plugin plugin, Runnable task, long delayTicks);
    BukkitTask runTaskTimer(Plugin plugin, Runnable task, long delayTicks, long periodTicks);
    /** Off the tick, on a thread of the scheduler's own.
     *
     * A task here must not touch the world. That is Bukkit's rule and it is a
     * hard one in Foton: the tick holds the locks, and a plugin reaching in
     * from another thread is the race the main-thread promise exists to stop.
     */
    BukkitTask runTaskAsynchronously(Plugin plugin, Runnable task);

    BukkitTask runTaskLaterAsynchronously(Plugin plugin, Runnable task, long delayTicks);

    BukkitTask runTaskTimerAsynchronously(
        Plugin plugin, Runnable task, long delayTicks, long periodTicks);

    /** Bukkit's oldest scheduling call, still written by plugins that have
     * been maintained for a decade. It is runTaskLater with an int for a
     * handle instead of a task. */
    int scheduleSyncDelayedTask(Plugin plugin, Runnable task, long delayTicks);

    int scheduleSyncDelayedTask(Plugin plugin, Runnable task);

    int scheduleSyncRepeatingTask(Plugin plugin, Runnable task, long delayTicks, long periodTicks);

    void cancelTask(int taskId);
    void cancelTasks(Plugin plugin);

    boolean isCurrentlyRunning(int taskId);
    default java.util.List<BukkitTask> getPendingTasks() { return java.util.Collections.emptyList(); }
    default java.util.List<BukkitWorker> getActiveWorkers() { return java.util.Collections.emptyList(); }
}
