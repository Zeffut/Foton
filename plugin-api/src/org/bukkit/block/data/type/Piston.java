package org.bukkit.block.data.type;

import org.bukkit.block.BlockFace;
import org.bukkit.block.data.Directional;

/** Directional piston base data. */
public interface Piston extends Directional {
    @Override void setFacing(BlockFace face);
    boolean isExtended();
    void setExtended(boolean extended);
}
