package io.papermc.paper.threadedregions.scheduler;

import java.util.function.Consumer;
import org.bukkit.plugin.Plugin;

/** Folia's scheduler for work that follows one entity.
 *
 * `retired` runs when the entity is gone before the task did. Foton runs
 * everything on the one tick, so a task here is a tick task and retirement is
 * never reached -- which is the same answer Paper gives on a server that is
 * not Folia.
 */
public interface EntityScheduler {
    boolean execute(Plugin plugin, Runnable task, Runnable retired, long delayTicks);

    ScheduledTask run(Plugin plugin, Consumer<ScheduledTask> task, Runnable retired);

    ScheduledTask runDelayed(
        Plugin plugin, Consumer<ScheduledTask> task, Runnable retired, long delayTicks);

    ScheduledTask runAtFixedRate(
        Plugin plugin,
        Consumer<ScheduledTask> task,
        Runnable retired,
        long delayTicks,
        long periodTicks);
}
