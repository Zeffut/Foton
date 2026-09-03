package foton;

import java.util.HashMap;
import org.bukkit.Material;
import org.bukkit.inventory.Inventory;
import org.bukkit.inventory.InventoryHolder;
import org.bukkit.inventory.ItemStack;

/** In-memory 27-slot inventory carried by a shulker item snapshot. */
final class FotonShulkerInventory implements Inventory {
    private final ItemStack[] slots = new ItemStack[27];
    private final InventoryHolder holder;
    FotonShulkerInventory() { this(null); }
    FotonShulkerInventory(InventoryHolder holder) { this.holder = holder; }
    @Override public InventoryHolder getHolder() { return holder; }
    @Override public int getSize() { return slots.length; }
    @Override public ItemStack getItem(int slot) { return slot < 0 || slot >= slots.length ? null : slots[slot]; }
    @Override public void setItem(int slot, ItemStack item) { if (slot >= 0 && slot < slots.length) slots[slot] = item == null ? null : item.clone(); }
    @Override
    public HashMap<Integer, ItemStack> addItem(ItemStack... items) {
        HashMap<Integer, ItemStack> left = new HashMap<>();
        if (items == null) return left;
        for (int index = 0; index < items.length; index++) {
            ItemStack item = items[index];
            if (item == null || item.getType().isAir()) continue;
            ItemStack remaining = item.clone();
            for (int slot = 0; slot < slots.length && remaining.getAmount() > 0; slot++) {
                ItemStack existing = slots[slot];
                if (existing == null || !existing.isSimilar(remaining)) continue;
                int room = Math.max(0, existing.getMaxStackSize() - existing.getAmount());
                int moved = Math.min(room, remaining.getAmount());
                if (moved > 0) {
                    existing.setAmount(existing.getAmount() + moved);
                    remaining.setAmount(remaining.getAmount() - moved);
                }
            }
            for (int slot = 0; slot < slots.length && remaining.getAmount() > 0; slot++) {
                ItemStack existing = slots[slot];
                if (existing != null && !existing.getType().isAir()) continue;
                int moved = Math.min(remaining.getAmount(), remaining.getMaxStackSize());
                ItemStack placed = remaining.clone();
                placed.setAmount(moved);
                slots[slot] = placed;
                remaining.setAmount(remaining.getAmount() - moved);
            }
            if (remaining.getAmount() > 0) left.put(index, remaining);
        }
        return left;
    }
    @Override public ItemStack[] getContents() { ItemStack[] copy = new ItemStack[slots.length]; for(int i=0;i<slots.length;i++) copy[i]=slots[i]==null?null:slots[i].clone(); return copy; }
    @Override public void setContents(ItemStack[] items) { for(int i=0;i<slots.length;i++) slots[i]=items!=null && i<items.length && items[i]!=null?items[i].clone():null; }
    @Override public boolean contains(Material material) { return first(material)>=0; }
    @Override public int first(Material material) { if(material==null)return -1; for(int i=0;i<slots.length;i++) if(slots[i]!=null&&slots[i].getType()==material)return i; return -1; }
    FotonShulkerInventory snapshot() { FotonShulkerInventory copy = new FotonShulkerInventory(holder); copy.setContents(getContents()); return copy; }
    @Override public void clear() { java.util.Arrays.fill(slots,null); }
    @Override public void clear(int slot) { if(slot>=0&&slot<slots.length)slots[slot]=null; }
}
