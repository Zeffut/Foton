package io.papermc.paper.plugin.lifecycle.event.handler.configuration;

import io.papermc.paper.plugin.lifecycle.event.LifecycleEvent;
import io.papermc.paper.plugin.lifecycle.event.handler.LifecycleEventHandler;
import io.papermc.paper.plugin.lifecycle.event.types.LifecycleEventType;

/** Event identity travels with a configured handler; priority and monitor are exclusive. */
public final class FotonHandlerConfiguration<T extends LifecycleEvent>
        implements PrioritizedLifecycleEventHandlerConfiguration<T> {
    private final LifecycleEventType<T> type;
    private final LifecycleEventHandler<T> handler;
    private int priority;
    private boolean monitor;
    public FotonHandlerConfiguration(LifecycleEventType<T> type, LifecycleEventHandler<T> handler) {
        this.type = java.util.Objects.requireNonNull(type);
        this.handler = java.util.Objects.requireNonNull(handler);
    }
    public LifecycleEventType<T> type() { return type; }
    public LifecycleEventHandler<T> handler() { return handler; }
    public int priority() { return priority; }
    public boolean isMonitor() { return monitor; }
    @Override public PrioritizedLifecycleEventHandlerConfiguration<T> priority(int value) {
        priority = value; monitor = false; return this;
    }
    @Override public PrioritizedLifecycleEventHandlerConfiguration<T> monitor() {
        priority = 0; monitor = true; return this;
    }
}
