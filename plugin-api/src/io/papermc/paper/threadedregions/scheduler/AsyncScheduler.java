package io.papermc.paper.threadedregions.scheduler;

import java.util.concurrent.TimeUnit;
import java.util.function.Consumer;
import org.bukkit.plugin.Plugin;

/** Folia's scheduler for work that must not be on the tick. */
public interface AsyncScheduler {
    ScheduledTask runNow(Plugin plugin, Consumer<ScheduledTask> task);

    ScheduledTask runDelayed(Plugin plugin, Consumer<ScheduledTask> task, long delay, TimeUnit unit);

    ScheduledTask runAtFixedRate(
        Plugin plugin, Consumer<ScheduledTask> task, long delay, long period, TimeUnit unit);

    void cancelTasks(Plugin plugin);
}
