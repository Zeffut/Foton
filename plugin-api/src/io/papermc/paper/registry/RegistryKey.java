package io.papermc.paper.registry;

import io.papermc.paper.registry.tag.TagKey;
import net.kyori.adventure.key.Key;
import net.kyori.adventure.key.Keyed;

/**
 * Identifies one of the registries exposed by Paper.
 *
 * <p>The implementation deliberately keeps the Adventure key as-is.  Paper
 * plugins pass these keys through to the server and a second, relocated Key
 * implementation would make registry keys impossible to compare.</p>
 */
public interface RegistryKey<T> extends Keyed {
    RegistryKey<org.bukkit.inventory.ItemType> ITEM = RegistryKeyImpl.create("item");
    RegistryKey<org.bukkit.enchantments.Enchantment> ENCHANTMENT = RegistryKeyImpl.create("enchantment");

    /** The Adventure key backing this registry key. */
    @Override
    Key key();

    default TypedKey<T> typedKey(Key key) {
        return TypedKey.create(this, key);
    }

    default TypedKey<T> typedKey(String key) {
        return TypedKey.create(this, key);
    }

    default TagKey<T> tagKey(Key key) {
        return TagKey.create(this, key);
    }

    default TagKey<T> tagKey(String key) {
        return TagKey.create(this, key);
    }
}

final class RegistryKeyImpl<T> implements RegistryKey<T> {
    private final Key key;

    private RegistryKeyImpl(Key key) {
        this.key = key;
    }

    static <T> RegistryKey<T> create(String value) {
        return new RegistryKeyImpl<>(Key.key("minecraft", value));
    }

    @Override
    public Key key() {
        return key;
    }

    @Override
    public String toString() {
        return key.asString();
    }

    // Registry keys are identity tokens in Paper, rather than registry values.
    @Override
    public boolean equals(Object other) {
        return this == other;
    }

    @Override
    public int hashCode() {
        return System.identityHashCode(this);
    }
}
