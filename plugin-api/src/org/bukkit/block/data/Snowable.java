package org.bukkit.block.data;

/** Block data that shows a snowy side: grass, podzol, mycelium. */
public interface Snowable extends BlockData {
    boolean isSnowy();
    void setSnowy(boolean snowy);
}
