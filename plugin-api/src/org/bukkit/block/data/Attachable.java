package org.bukkit.block.data;

/** Block data with vanilla's {@code attached} property: tripwires and their hooks. */
public interface Attachable extends BlockData {
    boolean isAttached();
    void setAttached(boolean attached);
}
