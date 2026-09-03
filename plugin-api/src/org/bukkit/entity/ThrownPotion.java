package org.bukkit.entity;
import org.bukkit.inventory.ItemStack;
/** A thrown potion projectile. */
public interface ThrownPotion extends Projectile {
    default ItemStack getItem() { return foton.FotonInventory.decode(foton.Native.entityItemStack(getUniqueId().toString())); }
    default java.util.Collection<org.bukkit.potion.PotionEffect> getEffects() {
        String[] values = foton.Native.entityPotionEffects(getUniqueId().toString());
        java.util.ArrayList<org.bukkit.potion.PotionEffect> result = new java.util.ArrayList<>();
        if (values == null) return result;
        for (String value : values) {
            if (value == null) continue;
            String[] parts = value.split("\\|", -1);
            if (parts.length < 6) continue;
            String name = parts[0];
            int separator = name.indexOf(':');
            if (separator >= 0) name = name.substring(separator + 1);
            try {
                org.bukkit.potion.PotionEffectType type = org.bukkit.potion.PotionEffectType.getByName(name);
                if (type != null) result.add(new org.bukkit.potion.PotionEffect(type, Integer.parseInt(parts[1]), Integer.parseInt(parts[2]), Boolean.parseBoolean(parts[3]), Boolean.parseBoolean(parts[4]), Boolean.parseBoolean(parts[5])));
            } catch (NumberFormatException ignored) { }
        }
        return result;
    }
}
