package org.bukkit.inventory.meta;

import java.util.List;

/** The name, lore and flags carried alongside an item.
 *
 * Bukkit's ItemMeta is a large interface with a dozen subtypes for particular
 * items. This is the part plugins actually reach for -- the display name and
 * the lore -- and it is honest about being that: a plugin asking for a
 * BookMeta gets nothing rather than a stub that quietly loses its pages.
 */
public interface ItemMeta extends Cloneable {
    default org.bukkit.persistence.PersistentDataContainer getPersistentDataContainer() {
        return new foton.FotonPersistentDataContainer();
    }
    boolean hasDisplayName();

    String getDisplayName();

    void setDisplayName(String name);

    boolean hasLore();

    List<String> getLore();

    void setLore(List<String> lore);

    boolean hasCustomModelData();

    int getCustomModelData();

    void setCustomModelData(Integer data);

    boolean isUnbreakable();

    void setUnbreakable(boolean unbreakable);

    boolean addEnchant(org.bukkit.enchantments.Enchantment enchantment, int level, boolean ignoreLevelRestriction);

    ItemMeta clone();
}
