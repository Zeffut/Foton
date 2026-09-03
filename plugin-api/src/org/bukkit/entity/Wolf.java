package org.bukkit.entity;

/** Vanilla wolf entity view. */
public interface Wolf extends Tameable {
    boolean isAngry();
    void setAngry(boolean angry);
    enum Variant { ASHEN, BLACK, CHESTNUT, PALE, RUSTY, SNOWY, SPOTTED, STRIPED, WOODS }
    Variant getVariant();
    void setVariant(Variant variant);
    boolean isSitting();
    void setSitting(boolean sitting);
    default org.bukkit.DyeColor getCollarColor() { return org.bukkit.DyeColor.values()[foton.Native.wolfCollarColor(((foton.FotonEntity) this).getUniqueId().toString())]; }
    default void setCollarColor(org.bukkit.DyeColor color) { if (color != null) foton.Native.setWolfCollarColor(((foton.FotonEntity) this).getUniqueId().toString(), color.ordinal()); }
}
