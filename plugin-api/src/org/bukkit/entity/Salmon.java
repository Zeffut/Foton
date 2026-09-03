package org.bukkit.entity;

/** Salmon entity view and its vanilla variant values. */
public interface Salmon extends Fish {
    enum Variant { TEMPERATE, COLD }
    default Variant getVariant() { return Variant.TEMPERATE; }
    default void setVariant(Variant variant) { }
}
