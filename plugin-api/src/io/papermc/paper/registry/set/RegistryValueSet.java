package io.papermc.paper.registry.set;

import java.util.Collection;
import java.util.Iterator;

/** A set of registry entries held directly, for registries whose entries are not keyed. */
public interface RegistryValueSet<T> extends Iterable<T>, RegistrySet<T> {
    @Override
    default int size() {
        return values().size();
    }

    Collection<T> values();

    @Override
    default Iterator<T> iterator() {
        return values().iterator();
    }
}
