package org.bukkit.block.data;

/** Block data with vanilla lit state. */
public interface Lightable extends BlockData {
    boolean isLit();
    void setLit(boolean lit);
}
