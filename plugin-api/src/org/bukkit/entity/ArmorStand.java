package org.bukkit.entity;

/** Vanilla armor stand entity. */
public interface ArmorStand extends LivingEntity {
    default void setArms(boolean arms) { }
}
