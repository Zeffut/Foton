package io.papermc.paper.plugin.lifecycle.event;

import java.util.ArrayList;
import java.util.List;
import io.papermc.paper.plugin.lifecycle.event.handler.LifecycleEventHandler;
import io.papermc.paper.plugin.lifecycle.event.handler.configuration.LifecycleEventHandlerConfiguration;
import io.papermc.paper.plugin.lifecycle.event.types.LifecycleEventType;

/** Dispatches handlers by priority, with stable registration order for ties. */
public final class FotonLifecycleEventManager implements LifecycleEventManager {
    private final List<Registration<?>> handlers = new ArrayList<>();
    private final boolean bootstrap;
    public FotonLifecycleEventManager() { this(false); }
    public FotonLifecycleEventManager(boolean bootstrap) { this.bootstrap = bootstrap; }
    public <T extends LifecycleEvent> void registerEventHandler(LifecycleEventType<T> type, LifecycleEventHandler<T> handler) {
        if (bootstrap && type != io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents.COMMANDS) {
            throw new UnsupportedOperationException("Foton has no transactional native bridge for this bootstrap lifecycle event");
        }
        handlers.add(new Registration<>(type, handler, 0, false));
    }
    public <T extends LifecycleEvent> void registerEventHandler(LifecycleEventHandlerConfiguration<T> configuration) {
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
    @SuppressWarnings("unchecked")
    public <T extends LifecycleEvent> void dispatch(LifecycleEventType<T> type, T event) {
        List<Registration<?>> ordered = new ArrayList<>(handlers);
        ordered.sort(java.util.Comparator.<Registration<?>, Boolean>comparing(Registration::monitor)
            .thenComparingInt(Registration::priority));
        for (Registration<?> registration : ordered) {
            if (registration.type == type) {
                ((LifecycleEventHandler<T>) registration.handler).run(event);
            }
        }
    }
    private record Registration<T extends LifecycleEvent>(LifecycleEventType<T> type, LifecycleEventHandler<T> handler,
                                                         int priority, boolean monitor) {}
}
