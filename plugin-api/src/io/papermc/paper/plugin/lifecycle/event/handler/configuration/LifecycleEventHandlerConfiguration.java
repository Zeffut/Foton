package io.papermc.paper.plugin.lifecycle.event.handler.configuration;

import io.papermc.paper.plugin.lifecycle.event.handler.LifecycleEventHandler;

public final class LifecycleEventHandlerConfiguration<T> {
    private final LifecycleEventHandler<T> handler;
    public LifecycleEventHandlerConfiguration(LifecycleEventHandler<T> handler) {
        this.handler = handler;
    }
    public LifecycleEventHandler<T> handler() { return handler; }
}
