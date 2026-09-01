package org.bukkit;

/** Sixteen by sixteen columns of a world. */
public interface Chunk {
    int getX();

    int getZ();

    World getWorld();
}
