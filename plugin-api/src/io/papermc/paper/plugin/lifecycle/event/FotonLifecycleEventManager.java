package io.papermc.paper.plugin.lifecycle.event;

import java.util.ArrayList;
import java.util.List;
import io.papermc.paper.plugin.lifecycle.event.handler.LifecycleEventHandler;
import io.papermc.paper.plugin.lifecycle.event.handler.configuration.LifecycleEventHandlerConfiguration;
import io.papermc.paper.plugin.lifecycle.event.types.LifecycleEventType;

/** Stores lifecycle handlers and dispatches them in registration order. */
public final class FotonLifecycleEventManager implements LifecycleEventManager {
    private final List<Registration<?>> handlers = new ArrayList<>();
    public <T extends LifecycleEvent> void registerEventHandler(LifecycleEventType<T> type, LifecycleEventHandler<T> handler) {
        handlers.add(new Registration<>(type, handler));
    }
    public <T extends LifecycleEvent> void registerEventHandler(LifecycleEventHandlerConfiguration<T> configuration) {
        handlers.add(new Registration<>(null, configuration.handler()));
    }
    @SuppressWarnings("unchecked")
    public <T extends LifecycleEvent> void dispatch(LifecycleEventType<T> type, T event) {
        for (Registration<?> registration : List.copyOf(handlers)) {
            if (registration.type == type || registration.type == null) {
                ((LifecycleEventHandler<T>) registration.handler).run(event);
            }
        }
    }
    private record Registration<T extends LifecycleEvent>(LifecycleEventType<T> type, LifecycleEventHandler<T> handler) {}
}
