package org.bukkit.entity;

/** A tameable parrot. */
public interface Parrot extends Tameable {
    enum Variant { RED, BLUE, GREEN, CYAN, GRAY }
    Variant getVariant();
    void setVariant(Variant variant);
}
