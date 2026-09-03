package org.bukkit;

/** Sixteen by sixteen columns of a world. */
public interface Chunk extends org.bukkit.persistence.PersistentDataHolder, org.bukkit.metadata.Metadatable {
    int getX();

    int getZ();

    World getWorld();

    org.bukkit.persistence.PersistentDataContainer getPersistentDataContainer();

    default org.bukkit.block.Block getBlock(int x, int y, int z) {
        return getWorld().getBlockAt(getX() * 16 + x, y, getZ() * 16 + z);
    }

    default org.bukkit.entity.Entity[] getEntities() { return new org.bukkit.entity.Entity[0]; }
    default org.bukkit.block.BlockState[] getTileEntities() { return new org.bukkit.block.BlockState[0]; }
    default org.bukkit.block.BlockState[] getTileEntities(boolean useSnapshot) { return getTileEntities(); }
    default java.util.Collection<org.bukkit.block.BlockState> getTileEntities(
            java.util.function.Predicate<org.bukkit.block.BlockState> filter, boolean useSnapshot) {
        java.util.ArrayList<org.bukkit.block.BlockState> result = new java.util.ArrayList<>();
        for (org.bukkit.block.BlockState state : getTileEntities())
            if (filter == null || filter.test(state)) result.add(state);
        return result;
    }
    default boolean isGenerated() { return getWorld().isChunkLoaded(getX(), getZ()); }
}
