package org.bukkit.entity;

/** A pig that can be equipped with a saddle. */
public interface Pig extends Animal, Ageable, Steerable {
    enum Variant { COLD, TEMPERATE, WARM }
    default Variant getVariant() {
        String value = foton.Native.pigVariant(((foton.FotonEntity) this).getUniqueId().toString());
        if (value == null) return Variant.TEMPERATE;
        try { return Variant.valueOf(value.toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException ignored) { return Variant.TEMPERATE; }
    }
    default void setVariant(Variant variant) {
        if (variant != null) foton.Native.setPigVariant(((foton.FotonEntity) this).getUniqueId().toString(), variant.name());
    }
    default boolean hasSaddle() {
        return foton.Native.pigHasSaddle(((foton.FotonEntity) this).getUniqueId().toString());
    }

    default void setSaddle(boolean saddled) {
        foton.Native.pigSetSaddle(((foton.FotonEntity) this).getUniqueId().toString(), saddled);
    }
}
