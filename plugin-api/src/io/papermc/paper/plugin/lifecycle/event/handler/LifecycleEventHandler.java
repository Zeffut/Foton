package io.papermc.paper.plugin.lifecycle.event.handler;

@FunctionalInterface
public interface LifecycleEventHandler<T extends io.papermc.paper.plugin.lifecycle.event.LifecycleEvent> {
    void run(T event);
}
