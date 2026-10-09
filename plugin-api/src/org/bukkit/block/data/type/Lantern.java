package org.bukkit.block.data.type;

import org.bukkit.block.data.Waterlogged;

public interface Lantern extends Waterlogged {
    boolean isHanging();
    void setHanging(boolean hanging);
}
