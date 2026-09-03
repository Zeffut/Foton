package org.bukkit.entity;

/** A rideable camel with the shared horse and taming APIs. */
public interface Camel extends AbstractHorse, Tameable {
    default org.bukkit.inventory.AbstractHorseInventory getInventory() { return null; }
}
