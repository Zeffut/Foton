package org.bukkit.block;

import org.bukkit.Location;
import org.bukkit.World;

/** One block in a world. */
public interface Block {
    int getX();

    int getY();

    int getZ();

    World getWorld();

    Location getLocation();
}
