package foton;

import java.util.HashMap;
import org.bukkit.Material;
import org.bukkit.inventory.Inventory;
import org.bukkit.inventory.InventoryHolder;
import org.bukkit.inventory.ItemStack;

/** Snapshot-backed facade for the top inventory of the player's open menu. */
public class FotonMenuInventory implements Inventory {
    private final String owner;
    private ItemStack[] snapshot;

    protected FotonMenuInventory(String owner) { this.owner = owner; }

    protected FotonMenuInventory(String owner, ItemStack[] snapshot) {
        this.owner = owner;
        this.snapshot = snapshot == null ? new ItemStack[0] : snapshot.clone();
    }

    @Override
    public InventoryHolder getHolder() {
        try { return new FotonPlayer(java.util.UUID.fromString(owner)); }
        catch (IllegalArgumentException ignored) { return null; }
    }

    @Override
    public int getSize() {
        if (snapshot != null) return snapshot.length;
        int size = Native.openMenuTopSlotCount(owner);
        return Math.max(0, size);
    }

    @Override public org.bukkit.event.inventory.InventoryType getType() {
        return switch (getSize()) {
            case 9 -> org.bukkit.event.inventory.InventoryType.GENERIC_9X1;
            case 18 -> org.bukkit.event.inventory.InventoryType.GENERIC_9X2;
            case 27 -> org.bukkit.event.inventory.InventoryType.GENERIC_9X3;
            case 36 -> org.bukkit.event.inventory.InventoryType.GENERIC_9X4;
            case 45 -> org.bukkit.event.inventory.InventoryType.GENERIC_9X5;
            case 54 -> org.bukkit.event.inventory.InventoryType.GENERIC_9X6;
            default -> org.bukkit.event.inventory.InventoryType.UNKNOWN;
        };
    }

    @Override
    public ItemStack getItem(int slot) {
        if (snapshot != null) return slot < 0 || slot >= snapshot.length || snapshot[slot] == null ? null : snapshot[slot].clone();
        return FotonInventory.decodeTransfer(Native.openMenuSlot(owner, slot));
    }

    @Override
    public void setItem(int slot, ItemStack item) {
        if (snapshot != null) { if (slot >= 0 && slot < snapshot.length) snapshot[slot] = item == null ? null : item.clone(); return; }
        Native.setOpenMenuItems(owner, new int[]{slot}, new foton.item.ItemMutation[]{FotonInventory.mutation(item)}, false);
    }

    @Override
    public HashMap<Integer, ItemStack> addItem(ItemStack... items) {
        HashMap<Integer, ItemStack> leftovers = new HashMap<>();
        if (items == null) return leftovers;
        ItemStack[] contents = getContents();
        for (int index = 0; index < items.length; index++) {
            ItemStack incoming = items[index] == null ? null : items[index].clone();
            if (incoming == null || incoming.getType().isAir() || incoming.getAmount() <= 0) continue;
            for (int slot = 0; slot < contents.length && incoming.getAmount() > 0; slot++) {
                ItemStack current = contents[slot];
                if (current != null && current.isSimilar(incoming)) {
                    int space = current.getMaxStackSize() - current.getAmount();
                    int moved = Math.min(Math.max(0, space), incoming.getAmount());
                    if (moved > 0) {
                        current.setAmount(current.getAmount() + moved);
                        incoming.setAmount(incoming.getAmount() - moved);
                        contents[slot] = current;
                    }
                }
            }
            for (int slot = 0; slot < contents.length && incoming.getAmount() > 0; slot++) {
                ItemStack current = contents[slot];
                if (current == null || current.getType().isAir() || current.getAmount() <= 0) {
                    int moved = Math.min(incoming.getMaxStackSize(), incoming.getAmount());
                    ItemStack placed = incoming.clone();
                    placed.setAmount(moved);
                    contents[slot] = placed;
                    incoming.setAmount(incoming.getAmount() - moved);
                }
            }
            if (incoming.getAmount() > 0) leftovers.put(index, incoming);
        }
        setContents(contents);
        return leftovers;
    }

    @Override
    public ItemStack[] getContents() {
        ItemStack[] result = new ItemStack[getSize()];
        for (int slot = 0; slot < result.length; slot++) result[slot] = getItem(slot);
        return result;
    }

    @Override
    public void setContents(ItemStack[] items) {
        int size = getSize();
        if (items != null && items.length > size) throw new IllegalArgumentException("inventory contents exceed size");
        if (snapshot != null) {
            ItemStack[] replacement = new ItemStack[size];
            for (int slot = 0; slot < size; slot++) replacement[slot] = items != null && slot < items.length && items[slot] != null ? items[slot].clone() : null;
            snapshot = replacement;
        } else setNativeContents(owner, size, items);
    }

    static void setNativeContents(String owner, int size, ItemStack[] items) {
        int[] slots = new int[size];
        for (int slot = 0; slot < size; slot++) slots[slot] = slot;
        Native.setOpenMenuItems(owner, slots, FotonInventory.mutations(size, items), true);
    }

    @Override
    public boolean contains(Material material) {
        return first(material) >= 0;
    }

    @Override
    public int first(Material material) {
        if (material == null) return -1;
        for (int slot = 0; slot < getSize(); slot++) {
            ItemStack item = getItem(slot);
            if (item != null && item.getType() == material) return slot;
        }
        return -1;
    }

    @Override
    public void clear() {
        setContents(null);
    }

    @Override
    public void clear(int slot) {
        setItem(slot, null);
    }
}
