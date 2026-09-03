package io.papermc.paper.plugin.lifecycle.event.types;

import io.papermc.paper.plugin.lifecycle.event.handler.LifecycleEventHandler;
import io.papermc.paper.plugin.lifecycle.event.handler.configuration.LifecycleEventHandlerConfiguration;

public interface LifecycleEventType<T extends io.papermc.paper.plugin.lifecycle.event.LifecycleEvent> {
    interface Prioritizable<T extends io.papermc.paper.plugin.lifecycle.event.LifecycleEvent> extends LifecycleEventType<T> {
        default LifecycleEventHandlerConfiguration<T> newHandler(LifecycleEventHandler<T> handler) {
            return new LifecycleEventHandlerConfiguration<>(handler);
        }
    }
}
