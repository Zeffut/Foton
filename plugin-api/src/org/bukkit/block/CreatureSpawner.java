package org.bukkit.block;

import org.bukkit.entity.EntityType;

/** Block state for a vanilla mob spawner. */
public interface CreatureSpawner extends TileState {
    EntityType getSpawnedType();
    void setSpawnedType(EntityType type);
    default int getDelay() { return 0; }
    default void setDelay(int delay) { }
    default int getMinSpawnDelay() { return 200; }
    default void setMinSpawnDelay(int delay) { }
    default int getMaxSpawnDelay() { return 800; }
    default void setMaxSpawnDelay(int delay) { }
}
