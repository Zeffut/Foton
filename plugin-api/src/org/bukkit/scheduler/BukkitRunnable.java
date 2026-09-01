package org.bukkit.scheduler;

import org.bukkit.Bukkit;
import org.bukkit.plugin.Plugin;

/** A stateful runnable that may be scheduled exactly once. */
public abstract class BukkitRunnable implements Runnable {
    private BukkitTask task;

    public synchronized boolean isCancelled() {
        checkScheduled();
        return task.isCancelled();
    }

    public synchronized void cancel() {
        checkScheduled();
        task.cancel();
    }

    public synchronized BukkitTask runTask(Plugin plugin) {
        checkNotYetScheduled();
        return setup(Bukkit.getScheduler().runTask(plugin, this));
    }

    public synchronized BukkitTask runTaskAsynchronously(Plugin plugin) {
        checkNotYetScheduled();
        return setup(Bukkit.getScheduler().runTaskAsynchronously(plugin, this));
    }

    public synchronized BukkitTask runTaskLater(Plugin plugin, long delay) {
        checkNotYetScheduled();
        return setup(Bukkit.getScheduler().runTaskLater(plugin, this, delay));
    }

    public synchronized BukkitTask runTaskLaterAsynchronously(Plugin plugin, long delay) {
        checkNotYetScheduled();
        return setup(Bukkit.getScheduler().runTaskLaterAsynchronously(plugin, this, delay));
    }

    public synchronized BukkitTask runTaskTimer(Plugin plugin, long delay, long period) {
        checkNotYetScheduled();
        return setup(Bukkit.getScheduler().runTaskTimer(plugin, this, delay, period));
    }

    public synchronized BukkitTask runTaskTimerAsynchronously(
            Plugin plugin, long delay, long period) {
        checkNotYetScheduled();
        return setup(Bukkit.getScheduler()
            .runTaskTimerAsynchronously(plugin, this, delay, period));
    }

    public synchronized int getTaskId() {
        checkScheduled();
        return task.getTaskId();
    }

    private BukkitTask setup(BukkitTask task) {
        this.task = task;
        return task;
    }

    private void checkScheduled() {
        if (task == null) {
            throw new IllegalStateException("Not scheduled yet");
        }
    }

    private void checkNotYetScheduled() {
        if (task != null) {
            throw new IllegalStateException("Already scheduled as " + task.getTaskId());
        }
    }
}
