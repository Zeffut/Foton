package org.bukkit.entity;

/** A cow. */
public interface Cow extends AbstractCow {
    interface Variant extends org.bukkit.Keyed {
        Variant COLD = () -> org.bukkit.NamespacedKey.minecraft("cold");
        Variant TEMPERATE = () -> org.bukkit.NamespacedKey.minecraft("temperate");
        Variant WARM = () -> org.bukkit.NamespacedKey.minecraft("warm");
    }

    interface SoundVariant extends org.bukkit.Keyed {
        SoundVariant CLASSIC = () -> org.bukkit.NamespacedKey.minecraft("classic");
        SoundVariant MOODY = () -> org.bukkit.NamespacedKey.minecraft("moody");
    }

    Variant getVariant();
    void setVariant(Variant variant);
    SoundVariant getSoundVariant();
    void setSoundVariant(SoundVariant variant);
}
