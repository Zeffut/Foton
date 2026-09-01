package io.papermc.paper.plugin.lifecycle.event.types;

import io.papermc.paper.plugin.lifecycle.event.handler.LifecycleEventHandler;
import io.papermc.paper.plugin.lifecycle.event.handler.configuration.LifecycleEventHandlerConfiguration;

public interface LifecycleEventType<T> {
    interface Prioritizable<T> extends LifecycleEventType<T> {
        default LifecycleEventHandlerConfiguration<T> newHandler(LifecycleEventHandler<T> handler) {
            return new LifecycleEventHandlerConfiguration<>(handler);
        }
    }
}
