package foton;

import java.util.concurrent.TimeUnit;
import java.util.function.Consumer;
import io.papermc.paper.threadedregions.scheduler.AsyncScheduler;
import io.papermc.paper.threadedregions.scheduler.EntityScheduler;
import io.papermc.paper.threadedregions.scheduler.GlobalRegionScheduler;
import io.papermc.paper.threadedregions.scheduler.RegionScheduler;
import io.papermc.paper.threadedregions.scheduler.ScheduledTask;
import org.bukkit.Location;
import org.bukkit.World;
import org.bukkit.plugin.Plugin;
import org.bukkit.scheduler.BukkitTask;

/** Folia's schedulers, answered by the one tick Foton has.
 *
 * A plugin written for Folia asks which region its work belongs to. Foton has
 * one, and it is the tick -- so every one of these runs its task exactly where
 * BukkitScheduler would. That is not a stub: it is the same answer Paper gives
 * on a server that is not Folia, and it is the guarantee the plugin asked for.
 */
public final class FotonRegionSchedulers {
    private FotonRegionSchedulers() {}

    static final GlobalRegionScheduler GLOBAL = new Global();
    static final RegionScheduler REGION = new Region();
    static final AsyncScheduler ASYNC = new Async();

    /** The scheduler that follows one entity, which is the same one tick. */
    public static EntityScheduler forEntity() {
        return new Entity();
    }

    /** A Folia task wrapping a Bukkit one, since they are the same task. */
    private record Task(Plugin owner, BukkitTask task) implements ScheduledTask {
        @Override public Plugin getOwningPlugin() {
            return owner;
        }

        @Override public boolean isCancelled() {
            return false;
        }

        @Override public void cancel() {
            task.cancel();
        }
    }

    /** Folia hands the task itself to the body; Bukkit does not. */
    private static ScheduledTask submit(
            Plugin plugin, Consumer<ScheduledTask> body, long delay, long period) {
        ScheduledTask[] handle = new ScheduledTask[1];
        Runnable runnable = () -> body.accept(handle[0]);
        BukkitTask task = period > 0
            ? new FotonScheduler().runTaskTimer(plugin, runnable, delay, period)
            : new FotonScheduler().runTaskLater(plugin, runnable, delay);
        handle[0] = new Task(plugin, task);
        return handle[0];
    }

    private static final class Global implements GlobalRegionScheduler {
        @Override public void execute(Plugin plugin, Runnable task) {
            new FotonScheduler().runTask(plugin, task);
        }

        @Override public ScheduledTask run(Plugin plugin, Consumer<ScheduledTask> task) {
            return submit(plugin, task, 0, -1);
        }

        @Override public ScheduledTask runDelayed(
                Plugin plugin, Consumer<ScheduledTask> task, long delayTicks) {
            return submit(plugin, task, delayTicks, -1);
        }

        @Override public ScheduledTask runAtFixedRate(
                Plugin plugin, Consumer<ScheduledTask> task, long delayTicks, long periodTicks) {
            return submit(plugin, task, delayTicks, periodTicks);
        }

        @Override public void cancelTasks(Plugin plugin) {
            new FotonScheduler().cancelTasks(plugin);
        }
    }

    private static final class Region implements RegionScheduler {
        @Override public void execute(Plugin plugin, Location location, Runnable task) {
            new FotonScheduler().runTask(plugin, task);
        }

        @Override public void execute(
                Plugin plugin, World world, int chunkX, int chunkZ, Runnable task) {
            new FotonScheduler().runTask(plugin, task);
        }

        @Override public ScheduledTask run(
                Plugin plugin, Location location, Consumer<ScheduledTask> task) {
            return submit(plugin, task, 0, -1);
        }

        @Override public ScheduledTask runDelayed(
                Plugin plugin, Location location, Consumer<ScheduledTask> task, long delayTicks) {
            return submit(plugin, task, delayTicks, -1);
        }

        @Override public ScheduledTask runAtFixedRate(
                Plugin plugin,
                Location location,
                Consumer<ScheduledTask> task,
                long delayTicks,
                long periodTicks) {
            return submit(plugin, task, delayTicks, periodTicks);
        }
    }

    private static final class Entity implements EntityScheduler {
        @Override public boolean execute(
                Plugin plugin, Runnable task, Runnable retired, long delayTicks) {
            new FotonScheduler().runTaskLater(plugin, task, delayTicks);
            return true;
        }

        @Override public ScheduledTask run(
                Plugin plugin, Consumer<ScheduledTask> task, Runnable retired) {
            return submit(plugin, task, 0, -1);
        }

        @Override public ScheduledTask runDelayed(
                Plugin plugin, Consumer<ScheduledTask> task, Runnable retired, long delayTicks) {
            return submit(plugin, task, delayTicks, -1);
        }

        @Override public ScheduledTask runAtFixedRate(
                Plugin plugin,
                Consumer<ScheduledTask> task,
                Runnable retired,
                long delayTicks,
                long periodTicks) {
            return submit(plugin, task, delayTicks, periodTicks);
        }
    }

    private static final class Async implements AsyncScheduler {
        @Override public ScheduledTask runNow(Plugin plugin, Consumer<ScheduledTask> task) {
            return async(plugin, task, 0, 0, TimeUnit.MILLISECONDS);
        }

        @Override public ScheduledTask runDelayed(
                Plugin plugin, Consumer<ScheduledTask> task, long delay, TimeUnit unit) {
            return async(plugin, task, delay, 0, unit);
        }

        @Override public ScheduledTask runAtFixedRate(
                Plugin plugin, Consumer<ScheduledTask> task, long delay, long period,
                TimeUnit unit) {
            return async(plugin, task, delay, period, unit);
        }

        @Override public void cancelTasks(Plugin plugin) {
            new FotonScheduler().cancelTasks(plugin);
        }

        /** Off the tick, which is what async means, in ticks the queue knows. */
        private static ScheduledTask async(
                Plugin plugin, Consumer<ScheduledTask> task, long delay, long period,
                TimeUnit unit) {
            long delayTicks = unit.toMillis(delay) / 50;
            long periodTicks = period <= 0 ? -1 : Math.max(1, unit.toMillis(period) / 50);
            ScheduledTask[] handle = new ScheduledTask[1];
            Runnable runnable = () -> task.accept(handle[0]);
            BukkitTask bukkit = periodTicks > 0
                ? new FotonScheduler().runTaskTimerAsynchronously(
                    plugin, runnable, delayTicks, periodTicks)
                : new FotonScheduler().runTaskLaterAsynchronously(plugin, runnable, delayTicks);
            handle[0] = new Task(plugin, bukkit);
            return handle[0];
        }
    }
}
