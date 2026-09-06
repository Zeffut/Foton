package com.destroystokyo.paper.profile;

import java.util.Collection;
import java.util.Set;

/** Paper profile alias retained for binary compatibility. */
public interface PlayerProfile extends org.bukkit.profile.PlayerProfile {
    default java.util.UUID getId() { return getUniqueId(); }
    default boolean completeFromCache() { return isComplete(); }

    /** The signed properties on this profile, chiefly {@code textures}.
     *
     * <p>Live, not a copy: Paper hands back the profile's own set and plugins
     * add to it directly, so returning a snapshot would make those additions
     * vanish without an error. */
    Set<ProfileProperty> getProperties();

    /** Adds a property, replacing any that share its name. */
    void setProperty(ProfileProperty property);

    /** Adds each property, replacing any that share a name. */
    void setProperties(Collection<ProfileProperty> properties);

    /** Removes every property with this name; returns whether any went. */
    boolean removeProperty(String name);

    default boolean hasProperty(String name) {
        for (ProfileProperty property : getProperties()) {
            if (property.getName().equals(name)) return true;
        }
        return false;
    }

    default void removeProperties(Collection<ProfileProperty> properties) {
        for (ProfileProperty property : properties) removeProperty(property.getName());
    }

    void clearProperties();
}
