package org.bukkit.entity;

/** A mooshroom cow. */
public interface MushroomCow extends Cow {
    enum Variant { RED, BROWN }
    Variant getVariant();
    void setVariant(Variant variant);
}
