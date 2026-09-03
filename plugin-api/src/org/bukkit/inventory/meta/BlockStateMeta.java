package org.bukkit.inventory.meta;

import org.bukkit.block.BlockState;

/** Item metadata carrying a block-state snapshot. */
public interface BlockStateMeta extends ItemMeta {
    default boolean hasBlockState() { return getBlockState() != null; }
    BlockState getBlockState();
    void setBlockState(BlockState state);
}
