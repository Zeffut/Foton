package org.bukkit.inventory.meta;

import org.bukkit.profile.PlayerProfile;

/** Metadata carried by player-head items. */
public interface SkullMeta extends ItemMeta {
    default boolean hasOwner() { return getOwnerProfile() != null; }
    default boolean setOwner(String name) {
        setOwnerProfile(name == null ? null : new foton.FotonPlayerProfile(null, name));
        return name != null;
    }
    /** Legacy owner-name accessor backed by the stored profile. */
    default String getOwner() {
        PlayerProfile profile = getOwnerProfile();
        return profile == null ? null : profile.getName();
    }
    /** A copy: edits reach the item only through {@link #setOwnerProfile}. */
    PlayerProfile getOwnerProfile();
    void setOwnerProfile(PlayerProfile profile);
    /** Paper's profile accessors, over the same stored profile. */
    default com.destroystokyo.paper.profile.PlayerProfile getPlayerProfile() {
        PlayerProfile profile = getOwnerProfile();
        return profile == null ? null : foton.FotonPlayerProfile.copyOf(profile);
    }
    default void setPlayerProfile(com.destroystokyo.paper.profile.PlayerProfile profile) { setOwnerProfile(profile); }
    default org.bukkit.OfflinePlayer getOwningPlayer() {
        PlayerProfile profile = getOwnerProfile();
        if (profile == null) return null;
        if (profile.getUniqueId() != null) return org.bukkit.Bukkit.getOfflinePlayer(profile.getUniqueId());
        return profile.getName() == null ? null : org.bukkit.Bukkit.getOfflinePlayer(profile.getName());
    }
    /** As Paper: an online player lends their whole profile, textures included. */
    default boolean setOwningPlayer(org.bukkit.OfflinePlayer player) {
        if (player == null) { setOwnerProfile(null); return true; }
        if (player instanceof org.bukkit.entity.Player online && online.getPlayerProfile() != null) {
            setOwnerProfile(online.getPlayerProfile());
            return true;
        }
        setOwnerProfile(new foton.FotonPlayerProfile(player.getUniqueId(), player.getName()));
        return true;
    }
}
