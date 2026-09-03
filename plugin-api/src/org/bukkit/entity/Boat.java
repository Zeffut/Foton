package org.bukkit.entity;

/** Bukkit boat marker and wood species enumeration. */
public interface Boat extends Vehicle {
    default Type getBoatType() { return Type.OAK; }
    default void setBoatType(Type type) { }
    default org.bukkit.TreeSpecies getWoodType() { return org.bukkit.TreeSpecies.GENERIC; }
    default void setWoodType(org.bukkit.TreeSpecies species) { }

    enum Type {
        OAK, SPRUCE, BIRCH, JUNGLE, ACACIA, DARK_OAK, MANGROVE, CHERRY, BAMBOO, PALE_OAK
    }
}
