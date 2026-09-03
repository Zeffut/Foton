package org.bukkit.block.data.type;

import org.bukkit.block.BlockFace;
import org.bukkit.block.data.Directional;

/** Vanilla bell block data. */
public interface Bell extends Directional {
    enum Attachment { FLOOR, CEILING, SINGLE_WALL, DOUBLE_WALL }
    Attachment getAttachment();
    void setAttachment(Attachment attachment);
}
