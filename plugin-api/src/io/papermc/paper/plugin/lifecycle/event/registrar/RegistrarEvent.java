package io.papermc.paper.plugin.lifecycle.event.registrar;

public interface RegistrarEvent<R extends Registrar>
        extends io.papermc.paper.plugin.lifecycle.event.LifecycleEvent {
    R registrar();
}
