package org.bukkit.entity;

/** Common tameable API shared by vanilla nautilus variants. */
public interface AbstractNautilus extends Tameable {
    org.bukkit.inventory.ArmoredSaddledMountInventory getInventory();
}
