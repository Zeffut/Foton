package org.bukkit.block.data.type;

import org.bukkit.block.BlockFace;
import org.bukkit.block.data.BlockData;

/** Shared data contract for pointed dripstone shapes. */
public interface Speleothem extends BlockData {
    enum Thickness { TIP_MERGE, TIP, FRUSTUM, MIDDLE, BASE }
    BlockFace getVerticalDirection();
    void setVerticalDirection(BlockFace direction);
    Thickness getThickness();
    void setThickness(Thickness thickness);
    boolean isWaterlogged();
    void setWaterlogged(boolean waterlogged);
}
