package foton;

import java.util.HashMap;
import org.bukkit.Material;
import org.bukkit.event.inventory.InventoryType;
import org.bukkit.inventory.Inventory;
import org.bukkit.inventory.InventoryHolder;
import org.bukkit.inventory.ItemStack;

/** Live 27-slot ender chest belonging to one player. */
public final class FotonEnderChestInventory implements Inventory {
    private final String owner;
    public FotonEnderChestInventory(String owner) { this.owner = owner; }
    @Override public InventoryHolder getHolder() { try { return new FotonPlayer(java.util.UUID.fromString(owner)); } catch (IllegalArgumentException e) { return null; } }
    @Override public int getSize() { return 27; }
    @Override public InventoryType getType() { return InventoryType.ENDER_CHEST; }
    @Override public ItemStack getItem(int slot) { return FotonInventory.decode(Native.enderChestSlot(owner, slot)); }
    @Override public void setItem(int slot, ItemStack item) { Native.setEnderChestSlot(owner, slot, FotonInventory.encode(item)); }
    @Override
    public HashMap<Integer, ItemStack> addItem(ItemStack... items) {
        HashMap<Integer, ItemStack> left = new HashMap<>();
        if (items == null) return left;
        ItemStack[] contents = getContents();
        for (int index = 0; index < items.length; index++) {
            ItemStack item = items[index];
            if (item == null || item.getType().isAir()) continue;
            ItemStack remaining = item.clone();
            for (int slot = 0; slot < contents.length && remaining.getAmount() > 0; slot++) {
                ItemStack existing = contents[slot];
                if (existing == null || !existing.isSimilar(remaining)) continue;
                int room = Math.max(0, existing.getMaxStackSize() - existing.getAmount());
                int moved = Math.min(room, remaining.getAmount());
                if (moved > 0) {
                    existing.setAmount(existing.getAmount() + moved);
                    remaining.setAmount(remaining.getAmount() - moved);
                }
            }
            for (int slot = 0; slot < contents.length && remaining.getAmount() > 0; slot++) {
                if (contents[slot] != null && !contents[slot].getType().isAir()) continue;
                int moved = Math.min(remaining.getAmount(), remaining.getMaxStackSize());
                ItemStack placed = remaining.clone();
                placed.setAmount(moved);
                contents[slot] = placed;
                remaining.setAmount(remaining.getAmount() - moved);
            }
            if (remaining.getAmount() > 0) left.put(index, remaining);
        }
        setContents(contents);
        return left;
    }
    @Override public ItemStack[] getContents() { ItemStack[] out = new ItemStack[getSize()]; for (int i = 0; i < out.length; i++) out[i] = getItem(i); return out; }
    @Override public void setContents(ItemStack[] items) { for (int i = 0; i < getSize(); i++) setItem(i, items != null && i < items.length ? items[i] : null); }
    @Override public boolean contains(Material material) { return first(material) >= 0; }
    @Override public int first(Material material) { if (material == null) return -1; for (int i = 0; i < getSize(); i++) if (getItem(i) != null && getItem(i).getType() == material) return i; return -1; }
    @Override public void clear() { for (int i = 0; i < getSize(); i++) clear(i); }
    @Override public void clear(int slot) { if (slot >= 0 && slot < getSize()) setItem(slot, null); }
}
