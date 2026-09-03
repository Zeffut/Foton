package org.bukkit.entity;

/** A creeper whose charged state can be controlled. */
public interface Creeper extends Monster {
    boolean isPowered();
    void setPowered(boolean powered);
    Entity getIgniter();
}
