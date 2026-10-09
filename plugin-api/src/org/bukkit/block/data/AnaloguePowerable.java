package org.bukkit.block.data;

/** Block data carrying a redstone power level from 0 to 15. */
public interface AnaloguePowerable extends BlockData {
    int getPower();
    void setPower(int power);
    int getMaximumPower();
}
