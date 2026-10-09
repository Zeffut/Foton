package org.bukkit.block.data;

/** Block data with vanilla's {@code hanging} property: lanterns. */
public interface Hangable extends BlockData {
    boolean isHanging();
    void setHanging(boolean hanging);
}
