package org.bukkit.entity;

/** A mooshroom cow. */
public interface MushroomCow extends AbstractCow {
    enum Variant { RED, BROWN }
    Variant getVariant();
    void setVariant(Variant variant);
}
