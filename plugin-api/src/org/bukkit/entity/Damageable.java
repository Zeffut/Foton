package org.bukkit.entity;
public interface Damageable extends Entity {
    double getHealth();
    void setHealth(double health);
    double getMaxHealth();
    default void setMaxHealth(double health) { }
}
