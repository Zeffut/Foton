package org.bukkit.block;

import org.bukkit.World;

/** A block, where it stands. */
public interface Block {
    int getX();
    int getY();
    int getZ();
    World getWorld();
}
