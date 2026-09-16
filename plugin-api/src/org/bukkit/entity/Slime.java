package org.bukkit.entity;

/** A size-changing vanilla slime. */
public interface Slime extends AbstractCubeMob {
    @Override int getSize();
    @Override
    void setSize(int size);
}
