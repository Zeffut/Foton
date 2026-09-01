package org.bukkit.block.data;

import org.bukkit.block.BlockFace;

/** Block data exposing vanilla's facing property. */
public interface Directional extends BlockData {
    BlockFace getFacing();
    void setFacing(BlockFace facing);
}
