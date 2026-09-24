package foton;

import java.util.HashMap;
import org.bukkit.Material;
import org.bukkit.inventory.Inventory;
import org.bukkit.inventory.InventoryHolder;
import org.bukkit.inventory.ItemStack;

/** The inventory a mob carries: a villager's eight slots, an allay's one.
 *
 * <p>Live, like every Foton inventory view: each call reads or writes the
 * entity's own container, so a plugin sees what the villager picked up since. */
final class FotonCarriedInventory implements Inventory {
    private final FotonEntity holder;

    FotonCarriedInventory(FotonEntity holder) { this.holder = holder; }

    private String id() { return holder.getUniqueId().toString(); }

    @Override public InventoryHolder getHolder() { return (InventoryHolder) holder; }
    @Override public int getSize() { return Math.max(0, Native.carriedInventorySize(id())); }
    @Override public ItemStack getItem(int slot) { return FotonInventory.decode(Native.carriedInventorySlot(id(), slot)); }
    @Override public void setItem(int slot, ItemStack item) { Native.setCarriedInventorySlot(id(), slot, FotonInventory.encode(item)); }

    @Override public HashMap<Integer, ItemStack> addItem(ItemStack... items) {
        HashMap<Integer, ItemStack> leftovers = new HashMap<>();
        if (items == null) return leftovers;
        int size = getSize();
        for (int index = 0; index < items.length; index++) {
            ItemStack incoming = items[index] == null ? null : items[index].clone();
            if (incoming == null || incoming.getType().isAir() || incoming.getAmount() <= 0) continue;
            // Top up matching stacks first, then fill empty slots: Bukkit's order.
            for (int slot = 0; slot < size && incoming.getAmount() > 0; slot++) {
                ItemStack current = getItem(slot);
                if (current == null || !current.isSimilar(incoming)) continue;
                int moved = Math.min(Math.max(0, current.getMaxStackSize() - current.getAmount()), incoming.getAmount());
                if (moved <= 0) continue;
                current.setAmount(current.getAmount() + moved);
                incoming.setAmount(incoming.getAmount() - moved);
                setItem(slot, current);
            }
            for (int slot = 0; slot < size && incoming.getAmount() > 0; slot++) {
                ItemStack current = getItem(slot);
                if (current != null && !current.getType().isAir() && current.getAmount() > 0) continue;
                ItemStack placed = incoming.clone();
                placed.setAmount(Math.min(incoming.getMaxStackSize(), incoming.getAmount()));
                setItem(slot, placed);
                incoming.setAmount(incoming.getAmount() - placed.getAmount());
            }
            if (incoming.getAmount() > 0) leftovers.put(index, incoming);
        }
        return leftovers;
    }

    @Override public ItemStack[] getContents() {
        ItemStack[] contents = new ItemStack[getSize()];
        for (int slot = 0; slot < contents.length; slot++) contents[slot] = getItem(slot);
        return contents;
    }
    @Override public void setContents(ItemStack[] items) {
        int size = getSize();
        for (int slot = 0; slot < size; slot++) setItem(slot, items != null && slot < items.length ? items[slot] : null);
    }
    @Override public boolean contains(Material material) { return first(material) >= 0; }
    @Override public int first(Material material) {
        if (material == null) return -1;
        int size = getSize();
        for (int slot = 0; slot < size; slot++) {
            ItemStack item = getItem(slot);
            if (item != null && item.getType() == material) return slot;
        }
        return -1;
    }
    @Override public void clear() { int size = getSize(); for (int slot = 0; slot < size; slot++) setItem(slot, null); }
    @Override public void clear(int slot) { setItem(slot, null); }
}
