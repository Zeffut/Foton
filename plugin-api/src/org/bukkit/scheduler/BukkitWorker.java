package org.bukkit.scheduler;

import org.bukkit.plugin.Plugin;

/** A currently executing scheduler task. */
public interface BukkitWorker {
    int getTaskId();
    Plugin getOwner();
    Thread getThread();
}
