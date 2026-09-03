package org.bukkit.inventory.meta;

import java.util.List;
import org.bukkit.inventory.ItemStack;

public interface CrossbowMeta extends ItemMeta {
    boolean hasChargedProjectiles();
    List<ItemStack> getChargedProjectiles();
    void setChargedProjectiles(List<ItemStack> projectiles);
    void addChargedProjectile(ItemStack projectile);
    default java.util.Map<String,Object> serialize() { return java.util.Collections.emptyMap(); }
}
