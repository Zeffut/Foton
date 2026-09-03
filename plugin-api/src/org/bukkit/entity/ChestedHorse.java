package org.bukkit.entity;

/** A horse-like entity that can carry a chest. */
public interface ChestedHorse extends LivingEntity {
    boolean isCarryingChest();
    void setCarryingChest(boolean carryingChest);
}
