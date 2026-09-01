package org.bukkit;

import java.util.UUID;

/** Somebody who has played here, whether or not they are here now. */
public interface OfflinePlayer {
    UUID getUniqueId();

    String getName();

    boolean isOnline();

    org.bukkit.entity.Player getPlayer();
    boolean hasPlayedBefore();
}
