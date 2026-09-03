package org.bukkit.entity;

import org.bukkit.inventory.EntityEquipment;

/** Living creature with equipment access. */
public interface Creature extends Mob {
    @Override EntityEquipment getEquipment();
}
