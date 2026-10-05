import io.papermc.paper.plugin.lifecycle.event.FotonLifecycleEventManager;
import io.papermc.paper.plugin.lifecycle.event.LifecycleEvent;
import io.papermc.paper.plugin.lifecycle.event.handler.LifecycleEventHandler;
import io.papermc.paper.plugin.lifecycle.event.handler.configuration.FotonHandlerConfiguration;
import io.papermc.paper.plugin.lifecycle.event.types.LifecycleEventType;
import java.util.ArrayList;
import java.util.List;

/** A late low-priority handler must neither be skipped nor replay an early handler. */
public final class LifecyclePriorityCheck {
    public static void main(String[] args) { check(); }
    public static void check() {
        FotonLifecycleEventManager manager = new FotonLifecycleEventManager();
        LifecycleEventType<LifecycleEvent> type = new LifecycleEventType<>() {};
        LifecycleEvent event = new LifecycleEvent() {};
        List<String> calls = new ArrayList<>();
        manager.registerEventHandler(new FotonHandlerConfiguration<>(type, ignored -> calls.add("early-high")).priority(10));
        manager.registerEventHandler(new FotonHandlerConfiguration<>(type, ignored -> calls.add("early-low")).priority(-10));
        Runnable phase = manager.incrementalDispatch(type, event);
        phase.run();
        same(calls, List.of("early-low", "early-high"));
        LifecycleEventHandler<LifecycleEvent> repeated = ignored -> calls.add("same-handler");
        manager.registerEventHandler(new FotonHandlerConfiguration<>(type, repeated).priority(-20));
        manager.registerEventHandler(new FotonHandlerConfiguration<>(type, repeated).priority(-20));
        manager.registerEventHandler(new FotonHandlerConfiguration<>(type, ignored -> {
            calls.add("late");
            manager.registerEventHandler(type, nested -> calls.add("nested"));
        }).priority(0));
        manager.registerEventHandler(new FotonHandlerConfiguration<>(type, ignored -> calls.add("monitor")).monitor());
        phase.run();
        same(calls, List.of("early-low", "early-high", "same-handler", "same-handler", "late", "monitor"));
        phase.run();
        same(calls, List.of("early-low", "early-high", "same-handler", "same-handler", "late", "monitor", "nested"));
        phase.run();
        if (calls.size() != 7) throw new AssertionError("delivered registrations replayed");
    }
    private static void same(Object actual, Object expected) {
        if (!expected.equals(actual)) throw new AssertionError("expected " + expected + ", got " + actual);
    }
}
