package io.papermc.paper.plugin.lifecycle.event.handler.configuration;

public interface PrioritizedLifecycleEventHandlerConfiguration<T extends io.papermc.paper.plugin.lifecycle.event.LifecycleEvent>
        extends LifecycleEventHandlerConfiguration<T> {
    PrioritizedLifecycleEventHandlerConfiguration<T> priority(int priority);
    PrioritizedLifecycleEventHandlerConfiguration<T> monitor();
}
