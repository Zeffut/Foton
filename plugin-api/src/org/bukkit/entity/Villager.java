package org.bukkit.entity;

/** A profession-bearing villager. */
public interface Villager extends AbstractVillager {
    enum Profession {
        ARMORER, BUTCHER, CARTOGRAPHER, CLERIC, FARMER, FISHERMAN, FLETCHER,
        LEATHERWORKER, LIBRARIAN, MASON, NITWIT, NONE, SHEPHERD, TOOLSMITH, WEAPONSMITH;
        public net.kyori.adventure.key.Key key() {
            return net.kyori.adventure.key.Key.key("minecraft:" + name().toLowerCase(java.util.Locale.ROOT));
        }
    }
    enum Type { DESERT, JUNGLE, PLAINS, SAVANNA, SNOW, SWAMP, TAIGA }
    default Profession getProfession() { return Profession.NONE; }
    default void setProfession(Profession profession) { }
    default int getVillagerExperience() { return 0; }
    default void setVillagerExperience(int experience) { }
    default int getVillagerLevel() { return 1; }
    default void setVillagerLevel(int level) { }
    /** Discards generated offers so they are rolled again from current data. */
    default void resetOffers() { }
    default <T> T getMemory(org.bukkit.entity.memory.MemoryKey<T> memoryKey) { return null; }
    default <T> void setMemory(org.bukkit.entity.memory.MemoryKey<T> memoryKey, T memory) { }
    default Type getVillagerType() { return Type.PLAINS; }
    default void setVillagerType(Type type) { }
}
