package org.bukkit.block.data.type;

import org.bukkit.block.data.Directional;
import org.bukkit.block.data.Lightable;
import org.bukkit.block.data.Waterlogged;

/** Campfire and soul campfire block data. */
public interface Campfire extends Directional, Lightable, Waterlogged {
    boolean isSignalFire();
    void setSignalFire(boolean signalFire);
}
