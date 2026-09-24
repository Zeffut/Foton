package io.papermc.paper.persistence;

import java.util.Set;
import org.bukkit.NamespacedKey;
import org.bukkit.persistence.PersistentDataAdapterContext;
import org.bukkit.persistence.PersistentDataContainer;
import org.bukkit.persistence.PersistentDataType;

/** The read half of a persistent data container, as Paper split it out.
 *
 * <p>{@code ItemStack#getPersistentDataContainer()} answers with one: reading
 * an item's data without building (and copying) its whole meta.</p>
 */
public interface PersistentDataContainerView {
    <P, C> boolean has(NamespacedKey key, PersistentDataType<P, C> type);

    boolean has(NamespacedKey key);

    <P, C> C get(NamespacedKey key, PersistentDataType<P, C> type);

    <P, C> C getOrDefault(NamespacedKey key, PersistentDataType<P, C> type, C defaultValue);

    Set<NamespacedKey> getKeys();

    boolean isEmpty();

    void copyTo(PersistentDataContainer other, boolean replace);

    PersistentDataAdapterContext getAdapterContext();

    /** The container as binary NBT, as Paper writes it: an unnamed root compound. */
    byte[] serializeToBytes() throws java.io.IOException;

    int getSize();
}
