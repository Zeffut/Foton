package org.bukkit.entity;

/** A goat, including its screaming variant. */
public interface Goat extends Animals, Ageable {
    boolean isScreaming();
    void setScreaming(boolean screaming);
    boolean hasLeftHorn();
    void setLeftHorn(boolean present);
    boolean hasRightHorn();
    void setRightHorn(boolean present);
}
