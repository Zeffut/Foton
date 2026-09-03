package org.bukkit.entity;

/** Vanilla ocelot entity view. */
public interface Ocelot extends Animal {
    enum Type { WILD, BLACK_CAT, RED_CAT, SIAMESE_CAT }
    default Type getCatType() { return Type.WILD; }
    default void setCatType(Type type) { }
    default boolean isBaby() { return foton.Native.entityIsBaby(((foton.FotonEntity) this).getUniqueId().toString()); }
    default void setBaby() { foton.Native.entitySetBaby(((foton.FotonEntity) this).getUniqueId().toString(), true); }
}
