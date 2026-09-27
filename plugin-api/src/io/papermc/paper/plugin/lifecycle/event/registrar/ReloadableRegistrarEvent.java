package io.papermc.paper.plugin.lifecycle.event.registrar;

public interface ReloadableRegistrarEvent<R extends Registrar> extends RegistrarEvent<R> {
    enum Cause { INITIAL, RELOAD }
    default Cause cause() { return Cause.INITIAL; }
}
