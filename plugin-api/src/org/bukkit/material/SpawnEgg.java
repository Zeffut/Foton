package org.bukkit.material;

import org.bukkit.Material;
import org.bukkit.entity.EntityType;
import org.bukkit.inventory.ItemStack;

/** Legacy spawn-egg material data facade. */
@Deprecated
public class SpawnEgg extends MaterialData {
    private EntityType spawnedType;

    public SpawnEgg(EntityType type) {
        super(materialFor(type));
        this.spawnedType = type == null ? EntityType.UNKNOWN : type;
    }

    public SpawnEgg(Material type) {
        super(type);
        this.spawnedType = type == null ? EntityType.UNKNOWN : findType(type);
    }

    public EntityType getSpawnedType() { return spawnedType; }
    public void setSpawnedType(EntityType type) { spawnedType = type == null ? EntityType.UNKNOWN : type; }

    /** Converts this legacy data object to a one-item stack. */
    public ItemStack toItemStack() { return new ItemStack(getItemType()); }

    private static Material materialFor(EntityType type) {
        if (type == null || type == EntityType.UNKNOWN) return Material.AIR;
        Material material = Material.matchMaterial(type.getName() + "_spawn_egg");
        return material == null ? Material.AIR : material;
    }

    private static EntityType findType(Material material) {
        String name = material.getKeyName();
        if (!name.endsWith("_spawn_egg")) return EntityType.UNKNOWN;
        String entity = name.substring(0, name.length() - "_spawn_egg".length());
        for (EntityType type : EntityType.values()) if (type.getName().equals(entity)) return type;
        return EntityType.UNKNOWN;
    }
}
