package org.bukkit.entity;

/** Vanilla cat entity view. */
public interface Cat extends Tameable {
    default boolean isBaby() { return foton.Native.entityIsBaby(((foton.FotonEntity) this).getUniqueId().toString()); }
    default void setBaby() { foton.Native.entitySetBaby(((foton.FotonEntity) this).getUniqueId().toString(), true); }
    enum Type { TABBY, BLACK, RED, SIAMESE, BRITISH_SHORTHAIR, CALICO, PERSIAN, RAGDOLL, WHITE, JELLIE, ALL_BLACK }
    Type getCatType();
    void setCatType(Type type);
    default boolean isSitting() { return false; }
    default void setSitting(boolean sitting) { }
    default org.bukkit.DyeColor getCollarColor() { return org.bukkit.DyeColor.RED; }
    default void setCollarColor(org.bukkit.DyeColor color) { }
}
