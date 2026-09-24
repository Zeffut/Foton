package org.bukkit.entity;

/** A player-shaped entity that wears a profile's skin and stands still. */
public interface Mannequin extends LivingEntity {
    io.papermc.paper.datacomponent.item.ResolvableProfile getProfile();

    void setProfile(io.papermc.paper.datacomponent.item.ResolvableProfile profile);

    /** Whether it ignores pushes and knockback. */
    boolean isImmovable();

    void setImmovable(boolean immovable);

    /** Sets the line shown under its name; null shows none. */
    void setDescription(net.kyori.adventure.text.Component description);
}
