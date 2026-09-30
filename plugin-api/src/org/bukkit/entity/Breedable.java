package org.bukkit.entity;

/** A creature that can breed, and whose age can be locked. */
public interface Breedable extends Ageable {
    @Override default boolean canBreed() { return Ageable.super.canBreed(); }
    @Override default void setBreed(boolean breed) { Ageable.super.setBreed(breed); }
}
