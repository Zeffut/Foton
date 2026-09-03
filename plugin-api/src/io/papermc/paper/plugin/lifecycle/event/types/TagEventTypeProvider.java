package io.papermc.paper.plugin.lifecycle.event.types;

import io.papermc.paper.plugin.lifecycle.event.registrar.ReloadableRegistrarEvent;
import io.papermc.paper.registry.RegistryKey;

/** The tag lifecycle events, one per registry. */
public interface TagEventTypeProvider {
    /** Fired once vanilla's tags for that registry are a plain list of entries. */
    <T> LifecycleEventType.Prioritizable<ReloadableRegistrarEvent> postFlatten(
        RegistryKey<T> registryKey);
}
