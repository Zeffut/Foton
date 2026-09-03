package org.bukkit.inventory.meta;

import java.util.ArrayList;
import java.util.List;
import java.util.Objects;
import org.bukkit.inventory.ItemStack;

/** In-memory Bukkit metadata for a bundle. */
public class SimpleBundleMeta extends SimpleItemMeta implements BundleMeta {
    private List<ItemStack> items = new ArrayList<>();

    @Override
    public List<ItemStack> getItems() {
        List<ItemStack> copy = new ArrayList<>(items.size());
        for (ItemStack item : items) {
            copy.add(item == null ? null : item.clone());
        }
        return copy;
    }

    @Override
    public void setItems(List<ItemStack> items) {
        this.items = new ArrayList<>();
        if (items == null) return;
        for (ItemStack item : items) {
            this.items.add(item == null ? null : item.clone());
        }
    }

        @Override
    public SimpleBundleMeta clone() {
        SimpleBundleMeta copy = (SimpleBundleMeta) super.clone();
        copy.setItems(items);
        return copy;
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof SimpleBundleMeta meta
            && super.equals(other)
            && Objects.equals(items, meta.items);
    }

    @Override
    public int hashCode() {
        return Objects.hash(super.hashCode(), items);
    }
}
