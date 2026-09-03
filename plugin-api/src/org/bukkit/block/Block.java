package org.bukkit.block;

import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.World;
import org.bukkit.block.data.BlockData;

/** One block in a world. */
public interface Block extends org.bukkit.metadata.Metadatable {
    default int getTypeId() { return getType().ordinal(); }
    default byte getData() { return 0; }
    default BlockFace getFace(Block block) { return null; }
    int getX();

    int getY();

    int getZ();

    World getWorld();

    default org.bukkit.Chunk getChunk() {
        return getWorld().getChunkAt(getX() >> 4, getZ() >> 4);
    }

    Location getLocation();

    Material getType();

    default Biome getBiome() { return null; }

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
    default PistonMoveReaction getPistonMoveReaction() { return PistonMoveReaction.NORMAL; }
    default boolean isPassable() { return foton.Native.blockPassable(getWorld().getName(), getX(), getY(), getZ()); }
    default byte getLightFromBlocks() { return 0; }
    default byte getLightFromSky() { return 0; }
    default boolean isBlockIndirectlyPowered() { return false; }
    default boolean breakNaturally() { return false; }

    Block getRelative(BlockFace face);

    Block getRelative(BlockFace face, int distance);

    Block getRelative(int x, int y, int z);
}
