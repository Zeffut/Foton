package org.bukkit.block.data;

/** Block data split into an upper and lower half. */
public interface Bisected extends BlockData {
    enum Half { TOP, BOTTOM }
    Half getHalf();
    void setHalf(Half half);
}
