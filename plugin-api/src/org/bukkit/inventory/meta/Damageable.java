package org.bukkit.inventory.meta;

/** Item metadata carrying vanilla durability damage. */
public interface Damageable extends ItemMeta {
    int getDamage();
    void setDamage(int damage);
    default boolean hasDamage() { return getDamage() != 0; }
}
