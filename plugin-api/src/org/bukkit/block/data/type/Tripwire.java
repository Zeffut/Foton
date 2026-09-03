package org.bukkit.block.data.type;

import org.bukkit.block.data.BlockData;

/** State properties specific to tripwire. */
public interface Tripwire extends BlockData {
    boolean isAttached();
    void setPowered(boolean powered);
}
