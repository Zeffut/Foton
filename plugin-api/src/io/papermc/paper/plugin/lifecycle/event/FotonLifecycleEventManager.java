package io.papermc.paper.plugin.lifecycle.event;

import java.util.ArrayList;
import java.util.List;
import io.papermc.paper.plugin.lifecycle.event.handler.LifecycleEventHandler;
import io.papermc.paper.plugin.lifecycle.event.handler.configuration.LifecycleEventHandlerConfiguration;
import io.papermc.paper.plugin.lifecycle.event.types.LifecycleEventType;

/** Dispatches handlers by priority, with stable registration order for ties. */
public final class FotonLifecycleEventManager implements LifecycleEventManager {
    private final List<Registration<?>> handlers = new ArrayList<>();
    private boolean transferred;
    private final boolean bootstrap;
    private boolean registrationOpen = true;
    public FotonLifecycleEventManager() { this(false); }
    public FotonLifecycleEventManager(boolean bootstrap) { this.bootstrap = bootstrap; }
    public synchronized void closeRegistration() { registrationOpen = false; }
    private void requireRegistrationOpen() {
        requireOpen();
        if (!registrationOpen) throw new IllegalStateException("Cannot register lifecycle event handlers");
    }
    public synchronized <T extends LifecycleEvent> void registerEventHandler(LifecycleEventType<T> type, LifecycleEventHandler<T> handler) {
        requireRegistrationOpen();
        if (bootstrap && type != io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents.COMMANDS) {
            throw new UnsupportedOperationException("Foton has no transactional native bridge for this bootstrap lifecycle event");
        }
        handlers.add(new Registration<>(type, handler, 0, false));
    }
    public synchronized <T extends LifecycleEvent> void registerEventHandler(LifecycleEventHandlerConfiguration<T> configuration) {
        requireRegistrationOpen();
        if (!(configuration instanceof io.papermc.paper.plugin.lifecycle.event.handler.configuration.FotonHandlerConfiguration<T> configured)) {
            throw new IllegalArgumentException("Unknown lifecycle handler configuration");
        }
        if (bootstrap && configured.type() != io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents.COMMANDS) {
            throw new UnsupportedOperationException("Foton has no transactional native bridge for this bootstrap lifecycle event");
        }
        if (bootstrap && (configured.priority() != 0 || configured.isMonitor())) {
            throw new UnsupportedOperationException("Cross-plugin bootstrap command priority ordering is not implemented");
        }
        handlers.add(new Registration<>(configured.type(), configured.handler(), configured.priority(), configured.isMonitor()));
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

    public <T extends LifecycleEvent> void dispatch(LifecycleEventType<T> type, T event) {
        incrementalDispatch(type, event).run();
    }

    /** Snapshot each phase; delivery identity survives priority changes between phases. */
    public <T extends LifecycleEvent> Runnable incrementalDispatch(LifecycleEventType<T> type, T event) {
        return new Runnable() {
            private final java.util.Set<Registration<?>> delivered =
                java.util.Collections.newSetFromMap(new java.util.IdentityHashMap<>());

            @Override @SuppressWarnings("unchecked")
            public void run() {
                List<Registration<?>> eligible;
                synchronized (FotonLifecycleEventManager.this) {
                    eligible = new ArrayList<>();
                    for (Registration<?> registration : handlers) {
                        if (registration.type == type && !delivered.contains(registration))
                            eligible.add(registration);
                    }
                }
                eligible.sort(java.util.Comparator.<Registration<?>, Boolean>comparing(Registration::monitor)
                    .thenComparingInt(Registration::priority));
                for (Registration<?> registration : eligible) {
                    // Consume before foreign code; reentrant delivery cannot replay it.
                    if (delivered.add(registration))
                        ((LifecycleEventHandler<T>) registration.handler).run(event);
                }
            }
        };
    }

    private void requireOpen() {
        if (transferred) {
            throw new IllegalStateException("lifecycle handlers were already transferred");
        }
    }
    private record Registration<T extends LifecycleEvent>(LifecycleEventType<T> type, LifecycleEventHandler<T> handler,
                                                         int priority, boolean monitor) {}
}
