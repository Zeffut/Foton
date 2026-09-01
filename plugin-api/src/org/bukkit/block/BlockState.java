package org.bukkit.block;

import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.World;
import org.bukkit.block.data.BlockData;

/** A block as it was when it was read.
 *
 * Bukkit's BlockState is a snapshot: reading one and then changing the world
 * does not change what the snapshot says, and `update` is what writes it back.
 * That is the contract plugins are written against, so it is the one here.
 */
public interface BlockState {
    default org.bukkit.persistence.PersistentDataContainer getPersistentDataContainer() {
        return new foton.FotonPersistentDataContainer();
    }
    Material getType();

    BlockData getBlockData();

    Block getBlock();

    Location getLocation();

    World getWorld();

    int getX();

    int getY();

    int getZ();

    /** Writes the snapshot back. False when it could not be written. */
    boolean update();

    boolean update(boolean force);
}
