package com.destroystokyo.paper.profile;

/** Paper profile alias retained for binary compatibility. */
public interface PlayerProfile extends org.bukkit.profile.PlayerProfile {
    default java.util.UUID getId() { return getUniqueId(); }
    default boolean completeFromCache() { return isComplete(); }
}
