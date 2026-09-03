package foton;

import java.util.UUID;

/** Live Bukkit view of a Steel zombie villager. */
public final class FotonZombieVillager extends FotonLivingEntity implements org.bukkit.entity.ZombieVillager {
    public FotonZombieVillager(UUID id) { super(id); }

    @Override public org.bukkit.entity.Villager.Profession getVillagerProfession() {
        String profession = Native.zombieVillagerProfession(getUniqueId().toString());
        if (profession == null) return org.bukkit.entity.Villager.Profession.NONE;
        try { return org.bukkit.entity.Villager.Profession.valueOf(profession.toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException ignored) { return org.bukkit.entity.Villager.Profession.NONE; }
    }
    @Override public void setVillagerProfession(org.bukkit.entity.Villager.Profession profession) {
        if (profession != null) Native.setZombieVillagerProfession(getUniqueId().toString(), profession.name());
    }

    @Override public org.bukkit.entity.Villager.Type getVillagerType() {
        String value = Native.villagerType(getUniqueId().toString());
        if (value == null) return org.bukkit.entity.Villager.Type.PLAINS;
        try { return org.bukkit.entity.Villager.Type.valueOf(value.toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException ignored) { return org.bukkit.entity.Villager.Type.PLAINS; }
    }
    @Override public void setVillagerType(org.bukkit.entity.Villager.Type type) {
        if (type != null) Native.setVillagerType(getUniqueId().toString(), type.name());
    }
}
