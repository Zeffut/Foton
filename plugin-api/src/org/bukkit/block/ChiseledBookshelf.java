package org.bukkit.block;

import org.bukkit.util.Vector;
import org.bukkit.inventory.ChiseledBookshelfInventory;

/** A six-slot chiseled bookshelf block state. */
public interface ChiseledBookshelf extends TileState, org.bukkit.inventory.BlockInventoryHolder {
    @Override ChiseledBookshelfInventory getInventory();
    default int getSlot(Vector position) {
        if (position == null) return -1;
        org.bukkit.block.data.BlockData raw = getBlockData();
        if (!(raw instanceof org.bukkit.block.data.Directional data)) return -1;
        org.bukkit.block.BlockFace face = data.getFacing();
        double u = switch (face) {
            case NORTH -> 1.0 - position.getX();
            case SOUTH -> position.getX();
            case WEST -> position.getZ();
            case EAST -> 1.0 - position.getZ();
            default -> -1.0;
        };
        if (u < 0.0 || u > 1.0 || position.getY() < 0.0 || position.getY() > 1.0) return -1;
        int column = Math.min(1, Math.max(0, (int) Math.floor(u * 2.0)));
        int row = Math.min(2, Math.max(0, (int) Math.floor((1.0 - position.getY()) * 3.0)));
        return column + row * 2;
    }
}
