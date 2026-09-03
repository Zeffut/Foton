package io.papermc.paper.registry.set;

import io.papermc.paper.registry.RegistryKey;
import io.papermc.paper.registry.TypedKey;
import java.util.ArrayList;
import java.util.Collection;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Objects;
import java.util.Set;
import org.bukkit.Keyed;

/** A set of registry entries, held either by key or by value. */
public interface RegistrySet<T> {
    /** A set holding the values themselves, for registries whose entries are not keyed. */
    static <T> RegistryValueSet<T> valueSet(RegistryKey<T> registryKey, Iterable<? extends T> values) {
        Objects.requireNonNull(registryKey, "registryKey");
        Objects.requireNonNull(values, "values");
        List<T> copy = new ArrayList<>();
        for (T value : values) copy.add(Objects.requireNonNull(value, "value"));
        return new RegistryValueSetImpl<>(registryKey, List.copyOf(copy));
    }

    /** A key set built from values, which is how a plugin names entries it already holds. */
    static <T extends Keyed> RegistryKeySet<T> keySetFromValues(RegistryKey<T> registryKey, Iterable<? extends T> values) {
        Objects.requireNonNull(registryKey, "registryKey");
        Objects.requireNonNull(values, "values");
        Set<TypedKey<T>> keys = new LinkedHashSet<>();
        for (T value : values) {
            Objects.requireNonNull(value, "value");
            keys.add(TypedKey.create(registryKey, value.getKey().toString()));
        }
        return new RegistryKeySetImpl<>(registryKey, List.copyOf(keys));
    }

    @SafeVarargs
    static <T extends Keyed> RegistryKeySet<T> keySet(RegistryKey<T> registryKey, TypedKey<T>... keys) {
        Objects.requireNonNull(keys, "keys");
        return keySet(registryKey, List.of(keys));
    }

    static <T extends Keyed> RegistryKeySet<T> keySet(RegistryKey<T> registryKey, Iterable<TypedKey<T>> keys) {
        Objects.requireNonNull(registryKey, "registryKey");
        Objects.requireNonNull(keys, "keys");
        Set<TypedKey<T>> copy = new LinkedHashSet<>();
        for (TypedKey<T> key : keys) copy.add(Objects.requireNonNull(key, "key"));
        return new RegistryKeySetImpl<>(registryKey, List.copyOf(copy));
    }

    /** The registry these entries belong to. */
    RegistryKey<T> registryKey();

    int size();

    default boolean isEmpty() {
        return size() == 0;
    }
}

final class RegistryKeySetImpl<T extends Keyed> implements RegistryKeySet<T> {
    private final RegistryKey<T> registryKey;
    private final Collection<TypedKey<T>> keys;

    RegistryKeySetImpl(RegistryKey<T> registryKey, Collection<TypedKey<T>> keys) {
        this.registryKey = registryKey;
        this.keys = keys;
    }

    @Override public RegistryKey<T> registryKey() { return registryKey; }
    @Override public Collection<TypedKey<T>> values() { return keys; }
    @Override public boolean contains(TypedKey<T> key) { return keys.contains(key); }

    @Override
    public Collection<T> resolve(org.bukkit.Registry<T> registry) {
        Objects.requireNonNull(registry, "registry");
        List<T> resolved = new ArrayList<>(keys.size());
        for (TypedKey<T> key : keys) {
            resolved.add(registry.getOrThrow(org.bukkit.NamespacedKey.fromString(key.key().asString())));
        }
        return List.copyOf(resolved);
    }

    @Override public String toString() { return "RegistryKeySet[" + registryKey + ", " + keys + "]"; }
}

final class RegistryValueSetImpl<T> implements RegistryValueSet<T> {
    private final RegistryKey<T> registryKey;
    private final Collection<T> values;

    RegistryValueSetImpl(RegistryKey<T> registryKey, Collection<T> values) {
        this.registryKey = registryKey;
        this.values = values;
    }

    @Override public RegistryKey<T> registryKey() { return registryKey; }
    @Override public Collection<T> values() { return values; }
    @Override public String toString() { return "RegistryValueSet[" + registryKey + ", " + values + "]"; }
}
