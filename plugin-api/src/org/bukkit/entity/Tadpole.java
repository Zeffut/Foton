package org.bukkit.entity;

/** Tadpole entity API. */
public interface Tadpole extends Ageable {
    default boolean isFromBucket() { return false; }
    default void setFromBucket(boolean fromBucket) { }
}
