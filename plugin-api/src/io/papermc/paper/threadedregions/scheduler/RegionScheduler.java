package io.papermc.paper.threadedregions.scheduler;

import java.util.function.Consumer;
import org.bukkit.Location;
import org.bukkit.World;
import org.bukkit.plugin.Plugin;

/** Folia's scheduler for work that belongs to one place in a world.
 *
 * Foton has one tick for every world, so where the work belongs makes no
 * difference to where it runs. The location is still taken, because a plugin
 * passes one and dropping the parameter would not compile.
 */
public interface RegionScheduler {
    void execute(Plugin plugin, Location location, Runnable task);

    void execute(Plugin plugin, World world, int chunkX, int chunkZ, Runnable task);

    ScheduledTask run(Plugin plugin, Location location, Consumer<ScheduledTask> task);

    default ScheduledTask run(Plugin plugin, World world, int chunkX, int chunkZ, Consumer<ScheduledTask> task) {
        if (world == null) return null;
        return run(plugin, world.getBlockAt(chunkX * 16, world.getMinHeight(), chunkZ * 16).getLocation(), task);
    }

    ScheduledTask runDelayed(
        Plugin plugin, Location location, Consumer<ScheduledTask> task, long delayTicks);

    default ScheduledTask runDelayed(Plugin plugin, World world, int chunkX, int chunkZ,
        Consumer<ScheduledTask> task, long delayTicks) {
        if (world == null) return null;
        return runDelayed(plugin, world.getBlockAt(chunkX * 16, world.getMinHeight(), chunkZ * 16).getLocation(), task, delayTicks);
    }

    ScheduledTask runAtFixedRate(
        Plugin plugin,
        Location location,
        Consumer<ScheduledTask> task,
        long delayTicks,
        long periodTicks);

    default ScheduledTask runAtFixedRate(Plugin plugin, World world, int chunkX, int chunkZ,
        Consumer<ScheduledTask> task, long delayTicks, long periodTicks) {
        if (world == null) return null;
        return runAtFixedRate(plugin, world.getBlockAt(chunkX * 16, world.getMinHeight(), chunkZ * 16).getLocation(), task, delayTicks, periodTicks);
    }
}
