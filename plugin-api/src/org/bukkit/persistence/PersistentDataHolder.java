package org.bukkit.persistence;

/** Object that exposes plugin-owned persistent data. */
public interface PersistentDataHolder {
    PersistentDataContainer getPersistentDataContainer();
}
