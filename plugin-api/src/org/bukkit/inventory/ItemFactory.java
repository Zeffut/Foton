package org.bukkit.inventory;

import org.bukkit.Material;
import org.bukkit.entity.EntityType;

/** Factory for Bukkit item representations. */
public interface ItemFactory {
    default org.bukkit.Color getDefaultLeatherColor() { return org.bukkit.Color.fromRGB(0xA06540); }
    default Material getSpawnEgg(EntityType type) {
        if (type == null) return null;
        return Material.matchMaterial(type.name().toLowerCase(java.util.Locale.ROOT) + "_spawn_egg");
    }
}
