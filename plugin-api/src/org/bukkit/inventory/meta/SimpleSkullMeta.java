package org.bukkit.inventory.meta;

import org.bukkit.profile.PlayerProfile;

/** In-memory Bukkit snapshot for a player head's profile. */
public final class SimpleSkullMeta extends SimpleItemMeta implements SkullMeta {
    private PlayerProfile profile;

    @Override
    public PlayerProfile getOwnerProfile() {
        return profile;
    }

    @Override
    public void setOwnerProfile(PlayerProfile value) {
        profile = value;
    }

    @Override
    public SimpleSkullMeta clone() {
        SimpleSkullMeta copy = (SimpleSkullMeta) super.clone();
        copy.profile = profile;
        return copy;
    }
}
