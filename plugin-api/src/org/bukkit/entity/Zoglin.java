package org.bukkit.entity;

/** Zoglin hostile entity. */
public interface Zoglin extends Monster, Ageable {
    default boolean isBaby() { return !isAdult(); }
}
