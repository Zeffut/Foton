package org.bukkit;

/** A registry-backed Bukkit value with a stable namespaced key. */
public interface Keyed extends net.kyori.adventure.key.Keyed {
    NamespacedKey getKey();

    @Override
    default net.kyori.adventure.key.Key key() {
        return getKey();
    }
}
