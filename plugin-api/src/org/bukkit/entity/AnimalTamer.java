package org.bukkit.entity;

import java.util.UUID;

/** Owner identity exposed by Bukkit tameable entities. */
public interface AnimalTamer {
    String getName();
    UUID getUniqueId();
}
