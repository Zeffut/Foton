package org.bukkit.block;

import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.World;
import org.bukkit.block.data.BlockData;

/** One block in a world. */
public interface Block {
    int getX();

    int getY();

    int getZ();

    World getWorld();

    default org.bukkit.Chunk getChunk() {
        return getWorld().getChunkAt(getX() >> 4, getZ() >> 4);
    }

    Location getLocation();

    Material getType();

    void setType(Material type);
    default void setType(Material type, boolean applyPhysics) { setType(type); }
    default void setBlockData(BlockData data) {
        if (data != null) setType(data.getMaterial());
    }
    default void setBlockData(BlockData data, boolean applyPhysics) { setBlockData(data); }

    BlockData getBlockData();

    BlockState getState();

    default BlockState getState(boolean useSnapshot) { return getState(); }

    boolean isEmpty();

    Block getRelative(BlockFace face);

    Block getRelative(BlockFace face, int distance);

    Block getRelative(int x, int y, int z);
}
