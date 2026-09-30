package org.bukkit.entity;

import org.bukkit.inventory.EntityEquipment;

/** Living creature with equipment access. */
public interface Creature extends Mob {
    /** Legacy Foton overload, inherited by Ageable without changing its Paper declarations. */
    default void setBaby(boolean baby) {
        if (!(this instanceof Ageable ageable)) {
            throw new UnsupportedOperationException("This creature is not ageable");
        }
        if (baby) ageable.setBaby(); else ageable.setAdult();
    }

    @Override EntityEquipment getEquipment();
}
