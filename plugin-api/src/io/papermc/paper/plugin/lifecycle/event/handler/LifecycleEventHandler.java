package io.papermc.paper.plugin.lifecycle.event.handler;

@FunctionalInterface
public interface LifecycleEventHandler<T> {
    void run(T event);
}
