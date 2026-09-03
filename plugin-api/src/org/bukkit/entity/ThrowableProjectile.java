package org.bukkit.entity;

import org.bukkit.inventory.ItemStack;

/** Projectile entity whose thrown item can be inspected or changed. */
public interface ThrowableProjectile extends Projectile {
    ItemStack getItem();
    void setItem(ItemStack item);
}
