package org.bukkit.entity;

/** A zombie carrying villager data. */
public interface ZombieVillager extends Ageable {
    default Villager.Profession getVillagerProfession() { return Villager.Profession.NONE; }
    default void setVillagerProfession(Villager.Profession profession) { }
    default Villager.Type getVillagerType() { return Villager.Type.PLAINS; }
    default void setVillagerType(Villager.Type type) { }
    default boolean isBaby() { return !isAdult(); }
    default void setBaby(boolean baby) { if (baby) setBaby(); else setAdult(); }
}
