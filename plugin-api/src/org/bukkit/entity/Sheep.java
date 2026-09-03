package org.bukkit.entity;

import org.bukkit.DyeColor;

/** Vanilla sheep entity view. */
public interface Sheep extends Animal, Ageable {
    DyeColor getColor();
    void setColor(DyeColor color);
    boolean isSheared();
    void setSheared(boolean sheared);
}
