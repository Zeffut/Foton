package org.bukkit;

import java.util.Arrays;
import java.util.Objects;
import java.util.stream.Stream;
import org.bukkit.enchantments.Enchantment;

/** Read-only Bukkit registry view backed by Steel's generated values. */
public interface Registry<T extends Keyed> {
    T get(NamespacedKey key);
    Stream<T> stream();

    default NamespacedKey getKeyOrThrow(T value) {
        return Objects.requireNonNull(value, "value").getKey();
    }

    Registry<Enchantment> ENCHANTMENT = new Registry<>() {
        @Override public Enchantment get(NamespacedKey key) {
            return Enchantment.getByKey(key);
        }
        @Override public Stream<Enchantment> stream() {
            return Arrays.stream(Enchantment.values());
        }
    };
}
