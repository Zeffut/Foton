package foton;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.UUID;
import org.bukkit.inventory.MerchantRecipe;

/** Live Bukkit view of a Steel villager and its generated offers. */
public final class FotonVillager extends AbstractVillager implements org.bukkit.entity.Villager, org.bukkit.entity.Ageable {
    public FotonVillager(UUID id) { super(id); }

    @Override public org.bukkit.entity.Villager.Type getVillagerType() {
        String type = Native.villagerType(getUniqueId().toString());
        if (type == null) return Type.PLAINS;
        try { return Type.valueOf(type.toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException ignored) { return Type.PLAINS; }
    }
    @Override public void setVillagerType(Type type) {
        if (type != null) Native.setVillagerType(getUniqueId().toString(), type.name());
    }

    @Override public Profession getProfession() {
        String profession = Native.villagerProfession(getUniqueId().toString());
        if (profession == null) return Profession.NONE;
        try { return Profession.valueOf(profession.toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException ignored) { return Profession.NONE; }
    }

    @Override public int getVillagerLevel() { return Native.villagerLevel(getUniqueId().toString()); }
    @Override public void setVillagerLevel(int level) { Native.setVillagerLevel(getUniqueId().toString(), level); }

    @Override public void setVillagerExperience(int experience) { Native.setVillagerExperience(getUniqueId().toString(), experience); }

    @Override public int getVillagerExperience() {
        return Native.villagerExperience(getUniqueId().toString());
    }

    @Override public void resetOffers() {
        Native.resetVillagerOffers(getUniqueId().toString());
    }

    @SuppressWarnings("unchecked")
    @Override public <T> T getMemory(org.bukkit.entity.memory.MemoryKey<T> memoryKey) {
        if (memoryKey == null) return null;
        String[] value = Native.villagerMemory(getUniqueId().toString(), memoryKey.getKey().getKey());
        if (value == null || value.length != 4 || memoryKey.getMemoryClass() != org.bukkit.Location.class) return null;
        try {
            return (T) new org.bukkit.Location(new FotonWorld(value[0]),
                Double.parseDouble(value[1]), Double.parseDouble(value[2]), Double.parseDouble(value[3]));
        } catch (NumberFormatException ignored) { return null; }
    }
    @Override public <T> void setMemory(org.bukkit.entity.memory.MemoryKey<T> memoryKey, T memory) {
        if (memoryKey == null || memory == null || memoryKey.getMemoryClass() != org.bukkit.Location.class) return;
        org.bukkit.Location location = (org.bukkit.Location) memory;
        if (location.getWorld() != null) Native.setVillagerMemory(getUniqueId().toString(), memoryKey.getKey().getKey(), location.getWorld().getName(), location.getBlockX(), location.getBlockY(), location.getBlockZ());
    }

    @Override public org.bukkit.entity.EntityType getType() {
        return org.bukkit.entity.EntityType.VILLAGER;
    }

    @Override public void setRecipes(List<MerchantRecipe> recipes) {
        if (recipes == null) { Native.setVillagerOffers(getUniqueId().toString(), new String[0]); return; }
        String[] encoded = new String[recipes.size()];
        for (int index = 0; index < recipes.size(); index++) encoded[index] = recipes.get(index) == null ? "" : recipes.get(index).encode();
        Native.setVillagerOffers(getUniqueId().toString(), encoded);
    }

    @Override public List<MerchantRecipe> getRecipes() {
        String[] encoded = Native.entityMerchantRecipes(getUniqueId().toString());
        if (encoded == null) return Collections.emptyList();
        ArrayList<MerchantRecipe> recipes = new ArrayList<>(encoded.length);
        for (int index = 0; index < encoded.length; index++) {
            String value = encoded[index];
            MerchantRecipe recipe = MerchantRecipe.decode(value, getUniqueId().toString(), index);
            if (recipe != null) recipes.add(recipe);
        }
        return Collections.unmodifiableList(recipes);
    }
}
