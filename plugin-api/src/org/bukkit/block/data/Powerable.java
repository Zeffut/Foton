package org.bukkit.block.data;

public interface Powerable extends BlockData {
    boolean isPowered();
    void setPowered(boolean powered);
}
