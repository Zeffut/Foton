package io.papermc.paper.plugin.lifecycle.event.types;

import io.papermc.paper.plugin.lifecycle.event.registrar.ReloadableRegistrarEvent;
import io.papermc.paper.registry.RegistryKey;

public final class LifecycleEvents {
    private LifecycleEvents() {}
    public static final LifecycleEventType.Prioritizable<ReloadableRegistrarEvent> COMMANDS = new Type<>();

    /** The tag lifecycle. `LifecycleEvents.TAGS.postFlatten(registry)` is how a
     * plugin says it wants to add to that registry's tags. */
    public static final TagEventTypeProvider TAGS = new Tags();

    private static final class Tags implements TagEventTypeProvider {
        // One event type per registry, so two plugins asking about the same
        // registry register against the same type and both get called.
        private final java.util.Map<RegistryKey<?>,
            LifecycleEventType.Prioritizable<ReloadableRegistrarEvent>> perRegistry =
                new java.util.concurrent.ConcurrentHashMap<>();

        @Override
        public <T> LifecycleEventType.Prioritizable<ReloadableRegistrarEvent> postFlatten(
                RegistryKey<T> registryKey) {
            return perRegistry.computeIfAbsent(registryKey, key -> new Type<>());
        }
    }
    private static final class Type<T extends io.papermc.paper.plugin.lifecycle.event.LifecycleEvent> implements LifecycleEventType.Prioritizable<T> {}
}
