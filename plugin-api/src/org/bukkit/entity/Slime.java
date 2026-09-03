package org.bukkit.entity;

/** A size-changing vanilla slime. */
public interface Slime extends LivingEntity {
    int getSize();
    void setSize(int size);
}
