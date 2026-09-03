package org.bukkit.entity;

/** End crystal entity. */
public interface EnderCrystal extends Entity {
    default boolean isShowingBottom() { return true; }
    default void setShowingBottom(boolean showing) { }
}
