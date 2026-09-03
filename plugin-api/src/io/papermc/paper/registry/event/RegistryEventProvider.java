package io.papermc.paper.registry.event;

import io.papermc.paper.plugin.lifecycle.event.registrar.ReloadableRegistrarEvent;
import io.papermc.paper.plugin.lifecycle.event.types.LifecycleEventType;
import io.papermc.paper.registry.RegistryBuilder;
import io.papermc.paper.registry.RegistryKey;

/** The lifecycle events of one registry. */
public final class RegistryEventProvider<T, B extends RegistryBuilder<T>> {
    private final RegistryKey<T> registryKey;
    private final LifecycleEventType.Prioritizable<ReloadableRegistrarEvent> freeze;

    RegistryEventProvider(RegistryKey<T> registryKey) {
        this.registryKey = registryKey;
        this.freeze = new FreezeType();
    }

    public RegistryKey<T> registryKey() {
        return registryKey;
    }

    /** Fired just before the registry closes. */
    public LifecycleEventType.Prioritizable<ReloadableRegistrarEvent> freeze() {
        return freeze;
    }

    private static final class FreezeType
            implements LifecycleEventType.Prioritizable<ReloadableRegistrarEvent> {}
}
