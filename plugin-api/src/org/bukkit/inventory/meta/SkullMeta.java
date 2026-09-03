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
    PlayerProfile getOwnerProfile();
    void setOwnerProfile(PlayerProfile profile);
    default boolean setOwningPlayer(org.bukkit.OfflinePlayer player) {
        if (player == null) { setOwnerProfile(null); return true; }
        setOwnerProfile(new foton.FotonPlayerProfile(player.getUniqueId(), player.getName()));
        return true;
    }
}
