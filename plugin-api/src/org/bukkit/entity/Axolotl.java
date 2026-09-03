package org.bukkit.entity;

/** A water-dwelling axolotl. */
public interface Axolotl extends Animal {
    enum Variant { LUCY, WILD, GOLD, CYAN, BLUE }
    Variant getVariant();
    void setVariant(Variant variant);
}
