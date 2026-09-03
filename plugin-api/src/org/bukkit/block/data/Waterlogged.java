package org.bukkit.block.data;

/** Block data for blocks that can contain water. */
public interface Waterlogged extends BlockData {
    boolean isWaterlogged();
    void setWaterlogged(boolean waterlogged);
}
