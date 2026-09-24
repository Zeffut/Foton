package org.bukkit.entity;

/** Something that can own a tamed animal: a player, online or not. */
public interface AnimalTamer {
    String getName();

    java.util.UUID getUniqueId();
}
