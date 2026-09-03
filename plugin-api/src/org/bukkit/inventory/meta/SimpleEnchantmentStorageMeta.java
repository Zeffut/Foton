package org.bukkit.inventory.meta;

import java.util.HashMap;
import java.util.Map;
import org.bukkit.enchantments.Enchantment;

/** In-memory implementation of enchanted-book metadata. */
public final class SimpleEnchantmentStorageMeta extends SimpleItemMeta implements EnchantmentStorageMeta {
    private Map<Enchantment, Integer> stored = new HashMap<>();

    @Override public boolean addStoredEnchant(Enchantment enchantment, int level, boolean ignoreLevelRestriction) {
        if (enchantment == null || level <= 0) return false;
        Integer previous = stored.put(enchantment, level);
        return previous == null || previous != level;
    }
    @Override public int getStoredEnchantLevel(Enchantment enchantment) { return stored.getOrDefault(enchantment, 0); }
    @Override public boolean hasStoredEnchant(Enchantment enchantment) { return stored.containsKey(enchantment); }
    @Override public boolean removeStoredEnchant(Enchantment enchantment) { return stored.remove(enchantment) != null; }
    @Override public Map<Enchantment, Integer> getStoredEnchants() {
        return java.util.Collections.unmodifiableMap(new HashMap<>(stored));
    }
    @Override public boolean hasConflictingStoredEnchant(Enchantment enchantment) { return false; }
    @Override public SimpleEnchantmentStorageMeta clone() {
        SimpleEnchantmentStorageMeta copy = (SimpleEnchantmentStorageMeta) super.clone();
        copy.stored = new HashMap<>(stored);
        return copy;
    }
    @Override public boolean equals(Object other) {
        return super.equals(other) && java.util.Objects.equals(stored, ((SimpleEnchantmentStorageMeta) other).stored);
    }
    @Override public int hashCode() { return java.util.Objects.hash(super.hashCode(), stored); }
}
