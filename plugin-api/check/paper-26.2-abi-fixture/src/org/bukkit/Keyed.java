package org.bukkit;

/** Minimal Paper 26.2 Keyed ABI fixture. */
public interface Keyed extends net.kyori.adventure.key.Keyed {
    NamespacedKey getKey();

    @Override
    default net.kyori.adventure.key.Key key() {
        return getKey();
    }
}
