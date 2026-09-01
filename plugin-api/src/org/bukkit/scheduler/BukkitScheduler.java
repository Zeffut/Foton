package org.bukkit.scheduler;

import org.bukkit.plugin.Plugin;

public interface BukkitScheduler {
    BukkitTask runTask(Plugin plugin, Runnable task);
    BukkitTask runTaskLater(Plugin plugin, Runnable task, long delayTicks);
    BukkitTask runTaskTimer(Plugin plugin, Runnable task, long delayTicks, long periodTicks);
    void cancelTask(int taskId);
    void cancelTasks(Plugin plugin);
}
