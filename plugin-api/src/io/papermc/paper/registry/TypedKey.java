package io.papermc.paper.registry;

import java.util.Objects;
import net.kyori.adventure.key.Key;

/**
 * Names one entry of one registry.
 *
 * <p>An interface, as in Paper, and not a class: a plugin compiled against
 * Paper calls {@link #create(RegistryKey, Key)} with {@code invokestatic} on an
 * interface method reference, and a class here would fail that call with an
 * {@code IncompatibleClassChangeError}.</p>
 */
public interface TypedKey<T> extends Key {
    /** The key itself, rather than this wrapper around it. */
    @Override
    Key key();

    /** The registry the entry belongs to. */
    RegistryKey<T> registryKey();

    static <T> TypedKey<T> create(RegistryKey<T> registryKey, Key key) {
        Objects.requireNonNull(registryKey, "registryKey");
        Objects.requireNonNull(key, "key");
        return new TypedKeyImpl<>(registryKey, key);
    }

    static <T> TypedKey<T> create(RegistryKey<T> registryKey, String key) {
        return create(registryKey, Key.key(key));
    }
}

final class TypedKeyImpl<T> implements TypedKey<T> {
    private final RegistryKey<T> registryKey;
    private final Key key;

    TypedKeyImpl(RegistryKey<T> registryKey, Key key) {
        this.registryKey = registryKey;
        this.key = key;
    }

    @Override public Key key() { return key; }
    @Override public RegistryKey<T> registryKey() { return registryKey; }
    @Override public String namespace() { return key.namespace(); }
    @Override public String value() { return key.value(); }
    @Override public String asString() { return key.asString(); }

    @Override
    public String toString() {
        return key.asString() + " (" + registryKey + ")";
    }

    @Override
    public boolean equals(Object other) {
        if (!(other instanceof TypedKey<?> that)) return false;
        return registryKey.equals(that.registryKey()) && key.equals(that.key());
    }

    @Override
    public int hashCode() {
        return 31 * registryKey.hashCode() + key.hashCode();
    }
}
