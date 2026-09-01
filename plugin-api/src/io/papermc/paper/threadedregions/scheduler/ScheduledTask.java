package io.papermc.paper.threadedregions.scheduler;

import org.bukkit.plugin.Plugin;

/** A task handed back by one of the region schedulers. */
public interface ScheduledTask {
    Plugin getOwningPlugin();

    boolean isCancelled();

    void cancel();
}
