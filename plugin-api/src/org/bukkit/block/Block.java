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

    Location getLocation();

    Material getType();

    void setType(Material type);

    BlockData getBlockData();

    BlockState getState();

    default BlockState getState(boolean useSnapshot) { return getState(); }

    boolean isEmpty();

    Block getRelative(BlockFace face);

    Block getRelative(BlockFace face, int distance);

    Block getRelative(int x, int y, int z);
}
