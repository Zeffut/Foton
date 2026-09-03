package org.bukkit;

import java.util.UUID;

/** Somebody who has played here, whether or not they are here now. */
public interface OfflinePlayer extends org.bukkit.configuration.serialization.ConfigurationSerializable {
    UUID getUniqueId();

    String getName();

    boolean isOnline();
    default boolean isOp() { return false; }
    default void setOp(boolean value) { }
    default boolean isWhitelisted() { return false; }
    default void setWhitelisted(boolean value) { }
    default boolean isBanned() { return false; }

    org.bukkit.entity.Player getPlayer();
    boolean hasPlayedBefore();

    long getFirstPlayed();
    long getLastPlayed();
    default int getStatistic(Statistic statistic) { return 0; }

    @Override default java.util.Map<String, Object> serialize() {
        java.util.Map<String, Object> data = new java.util.LinkedHashMap<>();
        if (getUniqueId() != null) data.put("UUID", getUniqueId().toString());
        if (getName() != null) data.put("name", getName());
        return data;
    }
}
