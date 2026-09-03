package io.papermc.paper.plugin.lifecycle.event.registrar;

public interface ReloadableRegistrarEvent extends io.papermc.paper.plugin.lifecycle.event.LifecycleEvent {
    Registrar registrar();
}
