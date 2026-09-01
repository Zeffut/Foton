package org.bukkit.scheduler;

import org.bukkit.plugin.Plugin;

public interface BukkitScheduler {
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

    void cancelTask(int taskId);
    void cancelTasks(Plugin plugin);
}
