package org.bukkit.entity;

/** A shulker: a golem fixed to a block face, peeking out of its shell. */
public interface Shulker extends Golem, org.bukkit.material.Colorable, Enemy {
    /** How far the shell is open, 0 (closed) to 1. */
    float getPeek();

    void setPeek(float value);

    org.bukkit.block.BlockFace getAttachedFace();

    void setAttachedFace(org.bukkit.block.BlockFace face);
}
