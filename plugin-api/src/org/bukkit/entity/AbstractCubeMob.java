package org.bukkit.entity;

/** Shared live state for cube mobs such as slimes. */
public interface AbstractCubeMob extends Creature {
    int getSize();
    void setSize(int size);
    boolean canWander();
    void setWander(boolean canWander);
}
