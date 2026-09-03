package org.bukkit.entity;

import org.bukkit.Location;

/** Immutable snapshot of an entity's type, world and position. */
public interface EntitySnapshot {
    Entity createEntity(Location location);
    String getAsString();
}
