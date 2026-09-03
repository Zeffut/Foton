package io.papermc.paper.plugin.lifecycle.event;

import io.papermc.paper.plugin.lifecycle.event.handler.LifecycleEventHandler;
import io.papermc.paper.plugin.lifecycle.event.handler.configuration.LifecycleEventHandlerConfiguration;
import io.papermc.paper.plugin.lifecycle.event.types.LifecycleEventType;

/** Paper lifecycle registration surface. */
public interface LifecycleEventManager {
    <T extends LifecycleEvent> void registerEventHandler(LifecycleEventType<T> type, LifecycleEventHandler<T> handler);
    <T extends LifecycleEvent> void registerEventHandler(LifecycleEventHandlerConfiguration<T> configuration);
    <T extends LifecycleEvent> void dispatch(LifecycleEventType<T> type, T event);
}
