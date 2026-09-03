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
    private static final class Task implements ScheduledTask {
        private final Plugin owner;
        private final boolean repeating;
        private volatile ExecutionState state = ExecutionState.IDLE;
        private BukkitTask task;

        private Task(Plugin owner, boolean repeating) {
            this.owner = owner;
            this.repeating = repeating;
        }

        private synchronized void bind(BukkitTask task) {
            this.task = task;
            if (isCancelled()) {
                task.cancel();
            }
        }

        private void run(Consumer<ScheduledTask> body) {
            synchronized (this) {
                if (state != ExecutionState.IDLE) {
                    return;
                }
                state = ExecutionState.RUNNING;
            }
            try {
                body.accept(this);
            } finally {
                synchronized (this) {
                    if (state == ExecutionState.CANCELLED_RUNNING) {
                        state = ExecutionState.CANCELLED;
                    } else {
                        state = repeating ? ExecutionState.IDLE : ExecutionState.FINISHED;
                    }
                }
            }
        }

        @Override public Plugin getOwningPlugin() {
            return owner;
        }

        @Override public boolean isRepeatingTask() {
            return repeating;
        }

        @Override public synchronized CancelledState cancel() {
            return switch (state) {
                case IDLE -> {
                    state = ExecutionState.CANCELLED;
                    if (task != null) {
                        task.cancel();
                    }
                    yield CancelledState.CANCELLED_BY_CALLER;
                }
                case RUNNING -> {
                    if (!repeating) {
                        yield CancelledState.RUNNING;
                    }
                    state = ExecutionState.CANCELLED_RUNNING;
                    if (task != null) {
                        task.cancel();
                    }
                    yield CancelledState.NEXT_RUNS_CANCELLED;
                }
                case FINISHED -> CancelledState.ALREADY_EXECUTED;
                case CANCELLED -> CancelledState.CANCELLED_ALREADY;
                case CANCELLED_RUNNING -> CancelledState.NEXT_RUNS_CANCELLED_ALREADY;
            };
        }

        @Override public ExecutionState getExecutionState() {
            return state;
        }
    }

    /** Folia hands the task itself to the body; Bukkit does not. */
    private static ScheduledTask submit(
            Plugin plugin, Consumer<ScheduledTask> body, long delay, long period) {
        Task handle = new Task(plugin, period > 0);
        Runnable runnable = () -> handle.run(body);
        BukkitTask task = period > 0
            ? new FotonScheduler().runTaskTimer(plugin, runnable, delay, period)
            : new FotonScheduler().runTaskLater(plugin, runnable, delay);
        handle.bind(task);
        return handle;
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
            Task handle = new Task(plugin, periodTicks > 0);
            Runnable runnable = () -> handle.run(task);
            BukkitTask bukkit = periodTicks > 0
                ? new FotonScheduler().runTaskTimerAsynchronously(
                    plugin, runnable, delayTicks, periodTicks)
                : new FotonScheduler().runTaskLaterAsynchronously(plugin, runnable, delayTicks);
            handle.bind(bukkit);
            return handle;
        }
    }
}
