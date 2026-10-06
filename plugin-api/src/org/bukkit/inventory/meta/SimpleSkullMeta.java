package org.bukkit.inventory.meta;

import org.bukkit.profile.PlayerProfile;

/** In-memory Bukkit snapshot for a player head's profile. */
public final class SimpleSkullMeta extends SimpleItemMeta implements SkullMeta {
    private foton.FotonPlayerProfile profile;

    @Override
    public foton.FotonPlayerProfile getOwnerProfile() {
        return profile == null ? null : profile.copy();
    }

    /** A profile naming neither an id, a name nor a property is no owner at all. */
    @Override
    public void setOwnerProfile(PlayerProfile value) {
        profile = value == null ? null : foton.FotonPlayerProfile.copyOf(value);
        if (profile != null && profile.getUniqueId() == null && profile.getName() == null
                && profile.getProperties().isEmpty()) profile = null;
        changed("profile", profile != null);
    }

    /** Hydration from native state; the journal is replaced once the bridge attaches it. */
    public void hydrateOwnerProfile(foton.FotonPlayerProfile value) { profile = value; }

    @Override
    public SimpleSkullMeta clone() {
        SimpleSkullMeta copy = (SimpleSkullMeta) super.clone();
        copy.profile = profile == null ? null : profile.copy();
        return copy;
    }

    @Override
    public boolean equals(Object other) {
        return super.equals(other) && java.util.Objects.equals(profile, ((SimpleSkullMeta) other).profile);
    }

    @Override
    public int hashCode() { return java.util.Objects.hash(super.hashCode(), profile); }
}
