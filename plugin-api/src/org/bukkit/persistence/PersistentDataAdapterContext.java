package org.bukkit.persistence;

/** What a {@link PersistentDataType} may need while converting: fresh containers. */
public interface PersistentDataAdapterContext {
    PersistentDataContainer newPersistentDataContainer();
}
