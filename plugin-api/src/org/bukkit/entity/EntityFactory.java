package org.bukkit.entity;

/** Factory for serializable Bukkit entity snapshots. */
public interface EntityFactory {
    EntitySnapshot createEntitySnapshot(String data);
}
