package io.papermc.paper.registry.set;

import io.papermc.paper.registry.TypedKey;
import java.util.Collection;
import java.util.Iterator;
import org.bukkit.Keyed;
import org.bukkit.Registry;

/** A set of registry entries named by key, which does not require them to exist yet. */
public interface RegistryKeySet<T extends Keyed> extends Iterable<TypedKey<T>>, RegistrySet<T> {
    @Override
    default int size() {
        return values().size();
    }

    Collection<TypedKey<T>> values();

    /** Looks every key up in {@code registry}; throws if one is absent. */
    Collection<T> resolve(Registry<T> registry);

    boolean contains(TypedKey<T> key);

    @Override
    default Iterator<TypedKey<T>> iterator() {
        return values().iterator();
    }
}
