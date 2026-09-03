package org.bukkit.persistence;

import java.util.Set;
import org.bukkit.NamespacedKey;

public interface PersistentDataContainer {
    <P, C> void set(NamespacedKey key, PersistentDataType<P, C> type, C value);
    <P, C> C get(NamespacedKey key, PersistentDataType<P, C> type);
    <P, C> C getOrDefault(NamespacedKey key, PersistentDataType<P, C> type, C fallback);
    <P, C> boolean has(NamespacedKey key, PersistentDataType<P, C> type);
    /** Returns whether any value is stored under this key. */
    default boolean has(NamespacedKey key) { return getKeys().contains(key); }
    void remove(NamespacedKey key);
    Set<NamespacedKey> getKeys();
}
