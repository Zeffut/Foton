package io.papermc.paper.threadedregions.scheduler;

import org.bukkit.plugin.Plugin;

/** A task handed back by one of the region schedulers. */
public interface ScheduledTask {
    Plugin getOwningPlugin();

    boolean isRepeatingTask();

    CancelledState cancel();

    ExecutionState getExecutionState();

    default boolean isCancelled() {
        ExecutionState state = getExecutionState();
        return state == ExecutionState.CANCELLED || state == ExecutionState.CANCELLED_RUNNING;
    }

    enum CancelledState {
        CANCELLED_BY_CALLER,
        CANCELLED_ALREADY,
        RUNNING,
        ALREADY_EXECUTED,
        NEXT_RUNS_CANCELLED,
        NEXT_RUNS_CANCELLED_ALREADY
    }

    enum ExecutionState {
        IDLE,
        RUNNING,
        FINISHED,
        CANCELLED,
        CANCELLED_RUNNING
    }
}
