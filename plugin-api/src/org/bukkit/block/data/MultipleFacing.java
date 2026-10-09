package org.bukkit.block.data;

import java.util.Set;
import org.bukkit.block.BlockFace;

/** Block data with independent faces, such as fences and vines. */
public interface MultipleFacing extends BlockData {
    Set<BlockFace> getFaces();
    Set<BlockFace> getAllowedFaces();
    boolean hasFace(BlockFace face);
    void setFace(BlockFace face, boolean has);
}
