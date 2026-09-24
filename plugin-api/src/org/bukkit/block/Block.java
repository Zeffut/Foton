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
    /** The light a mob-spawning or crop-growth check would read here:
     * whichever of sky and block light is brighter, which is how vanilla's
     * `getRawBrightness` combines them. */
    default byte getLightLevel() {
        return (byte) Math.max(getLightFromSky(), getLightFromBlocks());
    }

    default byte getLightFromBlocks() { return 0; }
    default byte getLightFromSky() { return 0; }
    default boolean isBlockIndirectlyPowered() { return false; }
    default boolean breakNaturally() { return false; }

    Block getRelative(BlockFace face);

    Block getRelative(BlockFace face, int distance);

    Block getRelative(int x, int y, int z);

    /** Whether the block is a liquid: water or lava. */
    default boolean isLiquid() { return (foton.Native.blockStateFlags(getBlockData().getAsString()) & 1) != 0; }

    /** The block's outline, in world coordinates; an empty box at the
     * block's corner when it has none. */
    default org.bukkit.util.BoundingBox getBoundingBox() {
        double[] boxes = foton.Native.blockShapeBoxes(getWorld().getName(), getX(), getY(), getZ(), 0);
        if (boxes == null || boxes.length < 6) return new org.bukkit.util.BoundingBox(getX(), getY(), getZ(), getX(), getY(), getZ());
        org.bukkit.util.BoundingBox bounds = new org.bukkit.util.BoundingBox(boxes[0], boxes[1], boxes[2], boxes[3], boxes[4], boxes[5]);
        for (int i = 6; i + 5 < boxes.length; i += 6)
            bounds.union(new org.bukkit.util.BoundingBox(boxes[i], boxes[i + 1], boxes[i + 2], boxes[i + 3], boxes[i + 4], boxes[i + 5]));
        return bounds.shift(getX(), getY(), getZ());
    }

    /** The block's collision shape, block-local. */
    default org.bukkit.util.VoxelShape getCollisionShape() {
        return new foton.FotonVoxelShape(foton.Native.blockShapeBoxes(getWorld().getName(), getX(), getY(), getZ(), 1));
    }
}
