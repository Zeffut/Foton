package org.bukkit.entity;

/** Vanilla zombie nautilus entity view. */
public interface ZombieNautilus extends Animal {
    enum Variant { TEMPERATE, WARM }
    default Variant getVariant() {
        String value = foton.Native.zombieNautilusVariant(((foton.FotonEntity) this).getUniqueId().toString());
        if (value == null) return Variant.TEMPERATE;
        try { return Variant.valueOf(value.toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException ignored) { return Variant.TEMPERATE; }
    }
    default void setVariant(Variant variant) {
        if (variant != null) foton.Native.setZombieNautilusVariant(((foton.FotonEntity) this).getUniqueId().toString(), variant.name());
    }
}
