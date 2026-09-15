package io.papermc.paper.plugin.lifecycle.event;

import java.util.ArrayList;
import java.util.List;
import io.papermc.paper.plugin.lifecycle.event.handler.LifecycleEventHandler;
import io.papermc.paper.plugin.lifecycle.event.handler.configuration.LifecycleEventHandlerConfiguration;
import io.papermc.paper.plugin.lifecycle.event.types.LifecycleEventType;

/** Stores lifecycle handlers and dispatches them in registration order. */
public final class FotonLifecycleEventManager implements LifecycleEventManager {
    private final List<Registration<?>> handlers = new ArrayList<>();
    private boolean transferred;

    public <T extends LifecycleEvent> void registerEventHandler(LifecycleEventType<T> type, LifecycleEventHandler<T> handler) {
        requireOpen();
        handlers.add(new Registration<>(type, handler));
    }
    public <T extends LifecycleEvent> void registerEventHandler(LifecycleEventHandlerConfiguration<T> configuration) {
        requireOpen();
        handlers.add(new Registration<>(null, configuration.handler()));
    }

    /** Moves bootstrap registrations ahead of handlers added by the plugin constructor. */
    public void transferTo(FotonLifecycleEventManager destination) {
        if (transferred) return;
        if (destination == this) {
            throw new IllegalArgumentException("cannot transfer lifecycle handlers to self");
        }
        List<Registration<?>> combined = new ArrayList<>(
            handlers.size() + destination.handlers.size());
        combined.addAll(handlers);
        combined.addAll(destination.handlers);
        destination.handlers.clear();
        destination.handlers.addAll(combined);
        handlers.clear();
        transferred = true;
    }

    @SuppressWarnings("unchecked")
    public <T extends LifecycleEvent> void dispatch(LifecycleEventType<T> type, T event) {
        for (Registration<?> registration : List.copyOf(handlers)) {
            if (registration.type == type || registration.type == null) {
                ((LifecycleEventHandler<T>) registration.handler).run(event);
            }
        }
    }

    private void requireOpen() {
        if (transferred) {
            throw new IllegalStateException("lifecycle handlers were already transferred");
        }
    }

    private record Registration<T extends LifecycleEvent>(LifecycleEventType<T> type, LifecycleEventHandler<T> handler) {}
}
