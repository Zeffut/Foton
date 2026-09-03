package org.bukkit.entity;

/** A frog with a biome-selected variant. */
public interface Frog extends Animal {
    enum Variant { TEMPERATE, WARM, COLD }
    Variant getVariant();
    void setVariant(Variant variant);
}
