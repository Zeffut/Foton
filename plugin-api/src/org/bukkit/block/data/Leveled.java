package org.bukkit.block.data;

/** Block data with a vanilla integer level property (fluids and powder snow). */
public interface Leveled extends BlockData {
    int getLevel();
    void setLevel(int level);
    int getMaximumLevel();
}
