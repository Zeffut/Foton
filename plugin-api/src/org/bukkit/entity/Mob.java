package org.bukkit.entity;

/** A living entity with a target. */
public interface Mob extends LivingEntity {
    LivingEntity getTarget();
    void setTarget(LivingEntity target);
}
