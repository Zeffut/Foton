package org.bukkit.entity;

/** An ageable entity whose breeding readiness can be controlled. */
public interface Breedable extends Ageable {
    boolean canBreed();
    void setBreed(boolean breed);
}
