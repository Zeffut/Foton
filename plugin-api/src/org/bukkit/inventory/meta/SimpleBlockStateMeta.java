package org.bukkit.inventory.meta;

import org.bukkit.block.BlockState;

/** Preserves block-state components without exposing an unimplemented mutable holder. */
public final class SimpleBlockStateMeta extends SimpleItemMeta implements BlockStateMeta {
    @Override public BlockState getBlockState() {
        throw new UnsupportedOperationException("block_state materialization");
    }
    @Override public void setBlockState(BlockState state) {
        throw new UnsupportedOperationException("block_state materialization");
    }
    @Override public SimpleBlockStateMeta clone() {
        return (SimpleBlockStateMeta) super.clone();
    }
}
