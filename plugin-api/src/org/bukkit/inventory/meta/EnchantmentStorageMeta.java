package org.bukkit.inventory.meta;

import java.util.Map;
import org.bukkit.enchantments.Enchantment;

/** Metadata for an enchanted book's stored enchantments. */
public interface EnchantmentStorageMeta extends ItemMeta {
    boolean addStoredEnchant(Enchantment enchantment, int level, boolean ignoreLevelRestriction);
    int getStoredEnchantLevel(Enchantment enchantment);
    boolean hasStoredEnchant(Enchantment enchantment);
    boolean removeStoredEnchant(Enchantment enchantment);
    Map<Enchantment, Integer> getStoredEnchants();
    boolean hasConflictingStoredEnchant(Enchantment enchantment);
}
