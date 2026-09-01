package org.bukkit.entity;

/** An entity with living characteristics. */
public interface LivingEntity extends Damageable {
    default org.bukkit.inventory.EntityEquipment getEquipment() { return null; }
    default boolean isPersistent() { return true; }
    default void setRemoveWhenFarAway(boolean remove) { }
}
