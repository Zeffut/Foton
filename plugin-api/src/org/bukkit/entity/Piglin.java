package org.bukkit.entity;

/** Piglin entity API. */
public interface Piglin extends Ageable {
    default boolean isBaby() { return !isAdult(); }
}
