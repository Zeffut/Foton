package io.papermc.paper.registry.tag;

import io.papermc.paper.registry.RegistryKey;
import net.kyori.adventure.key.Key;
import net.kyori.adventure.key.Keyed;

/** Identifies a tag in a particular registry. */
public interface TagKey<T> extends Keyed {
    static <T> TagKey<T> create(RegistryKey<T> registryKey, Key key) {
        if (registryKey == null || key == null) {
            throw new NullPointerException("registryKey and key");
        }
        return new TagKeyImpl<>(registryKey, key);
    }

    static <T> TagKey<T> create(RegistryKey<T> registryKey, String key) {
        return create(registryKey, Key.key(key));
    }

    RegistryKey<T> registryKey();
}

final class TagKeyImpl<T> implements TagKey<T> {
    private final RegistryKey<T> registryKey;
    private final Key key;

    TagKeyImpl(RegistryKey<T> registryKey, Key key) {
        this.registryKey = registryKey;
        this.key = key;
    }

    @Override public RegistryKey<T> registryKey() { return registryKey; }
    @Override public Key key() { return key; }

    @Override
    public String toString() {
        return key + " (" + registryKey + ")";
    }

    @Override
    public boolean equals(Object other) {
        if (!(other instanceof TagKey<?> that)) return false;
        return registryKey.equals(that.registryKey()) && key.equals(that.key());
    }

    @Override
    public int hashCode() {
        return 31 * registryKey.hashCode() + key.hashCode();
    }
}
