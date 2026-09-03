package io.papermc.paper.threadedregions.scheduler;

import java.util.function.Consumer;
import org.bukkit.plugin.Plugin;

/** Folia's scheduler for work that belongs to no particular region.
 *
 * Foton is not region-threaded, so there is one region and it is the tick.
 * Every method here runs its task exactly where BukkitScheduler would, which
 * is also what Paper does on a server that is not Folia -- a plugin written
 * for Folia gets the guarantee it asked for.
 */
public interface GlobalRegionScheduler {
    void execute(Plugin plugin, Runnable task);

    ScheduledTask run(Plugin plugin, Consumer<ScheduledTask> task);

    ScheduledTask runDelayed(Plugin plugin, Consumer<ScheduledTask> task, long delayTicks);

    ScheduledTask runAtFixedRate(
        Plugin plugin, Consumer<ScheduledTask> task, long delayTicks, long periodTicks);

    void cancelTasks(Plugin plugin);
}
