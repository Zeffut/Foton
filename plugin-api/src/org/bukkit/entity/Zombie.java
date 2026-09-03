package org.bukkit.entity;

/** A hostile undead zombie. */
public interface Zombie extends Monster, Ageable {
    /** Converts this zombie into a zombie villager when enabled. */
    default void setVillager(boolean villager) {
        foton.Native.setZombieVillager(getUniqueId().toString(), villager);
    }

    default boolean isBaby() {
        return !isAdult();
    }

    default void setBaby(boolean baby) {
        if (baby) setBaby();
        else setAdult();
    }
}
