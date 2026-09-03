package org.bukkit;

/** A registry-backed Bukkit value with a stable namespaced key. */
public interface Keyed {
    NamespacedKey getKey();
}
