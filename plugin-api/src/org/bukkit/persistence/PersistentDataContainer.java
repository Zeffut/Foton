package org.bukkit.persistence;

import java.util.Set;
import org.bukkit.NamespacedKey;

public interface PersistentDataContainer {
    <P, C> void set(NamespacedKey key, PersistentDataType<P, C> type, C value);
    <P, C> C get(NamespacedKey key, PersistentDataType<P, C> type);
    <P, C> C getOrDefault(NamespacedKey key, PersistentDataType<P, C> type, C fallback);
    <P, C> boolean has(NamespacedKey key, PersistentDataType<P, C> type);
    void remove(NamespacedKey key);
    Set<NamespacedKey> getKeys();
}
