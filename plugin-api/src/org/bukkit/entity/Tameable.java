package org.bukkit.entity;

/** An animal whose ownership state is backed by the live entity. */
public interface Tameable extends Animal {
    boolean isTamed();
    void setTamed(boolean tamed);
    AnimalTamer getOwner();
    void setOwner(AnimalTamer owner);
    default java.util.UUID getOwnerUniqueId() {
        AnimalTamer owner = getOwner();
        return owner == null ? null : owner.getUniqueId();
    }
}
