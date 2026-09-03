package io.papermc.paper.plugin.lifecycle.event.types;

import io.papermc.paper.plugin.lifecycle.event.registrar.ReloadableRegistrarEvent;

public final class LifecycleEvents {
    private LifecycleEvents() {}
    public static final LifecycleEventType.Prioritizable<ReloadableRegistrarEvent> COMMANDS = new Type<>();
    private static final class Type<T extends io.papermc.paper.plugin.lifecycle.event.LifecycleEvent> implements LifecycleEventType.Prioritizable<T> {}
}
