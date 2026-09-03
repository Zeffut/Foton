package org.bukkit.entity;

/** A lightning bolt entity. */
public interface LightningStrike extends Entity {
    /** Returns the entity that caused this strike, when one was recorded. */
    default Entity getCausingEntity() { return null; }
}
