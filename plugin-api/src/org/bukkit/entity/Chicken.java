package org.bukkit.entity;

/** Vanilla chicken entity view. */
public interface Chicken extends Animal {
    enum Variant { COLD, TEMPERATE, WARM }
    Variant getVariant();
    void setVariant(Variant variant);
}
