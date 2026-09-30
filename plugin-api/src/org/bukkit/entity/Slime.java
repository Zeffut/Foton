package org.bukkit.entity;

/** A size-changing vanilla slime. */
public interface Slime extends AbstractCubeMob, Enemy {
    void setSize(int size);
}
