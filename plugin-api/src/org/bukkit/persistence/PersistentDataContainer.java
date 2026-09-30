package org.bukkit.persistence;

import org.bukkit.NamespacedKey;

/** Typed custom data stored on an item, entity, chunk or block. */
public interface PersistentDataContainer extends io.papermc.paper.persistence.PersistentDataContainerView {
    <P, C> void set(NamespacedKey key, PersistentDataType<P, C> type, C value);

    void remove(NamespacedKey key);

    /** Replaces (or, when {@code clear} is false, adds to) this container's contents from binary NBT. */
    void readFromBytes(byte[] bytes, boolean clear) throws java.io.IOException;

    default void readFromBytes(byte[] bytes) throws java.io.IOException {
        readFromBytes(bytes, true);
    }
}
