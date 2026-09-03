package org.bukkit.inventory.meta;

import org.bukkit.block.BlockState;

/** In-memory block-state item metadata. */
public final class SimpleBlockStateMeta extends SimpleItemMeta implements BlockStateMeta {
    private BlockState state;
    @Override public BlockState getBlockState() { if (state == null) state = new foton.FotonShulkerBox(); return state; }
    @Override public void setBlockState(BlockState state) { this.state = state; }
    @Override public SimpleBlockStateMeta clone() {
        SimpleBlockStateMeta copy = new SimpleBlockStateMeta();
        copy.state = state;
        return copy;
    }
}
