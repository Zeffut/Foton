package foton;

import java.util.HashMap;
import org.bukkit.Material;
import org.bukkit.inventory.ChiseledBookshelfInventory;
import org.bukkit.inventory.InventoryHolder;
import org.bukkit.inventory.ItemStack;

/** Live six-slot view of a chiseled bookshelf. */
final class FotonChiseledBookshelfInventory implements ChiseledBookshelfInventory {
    private final FotonChiseledBookshelf holder;
    FotonChiseledBookshelfInventory(FotonChiseledBookshelf holder) { this.holder = holder; }
    private String world() { return holder.getWorld().getName(); }
    @Override public InventoryHolder getHolder() { return holder; }
    @Override public ItemStack getItem(int slot) { return slot < 0 || slot >= 6 ? null : FotonInventory.decode(Native.hopperInventorySlot(world(), holder.getX(), holder.getY(), holder.getZ(), slot)); }
    @Override public void setItem(int slot, ItemStack item) { if (slot >= 0 && slot < 6) Native.hopperSetInventorySlot(world(), holder.getX(), holder.getY(), holder.getZ(), slot, FotonInventory.encode(item)); }
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
    @Override public ItemStack[] getContents() { ItemStack[] out=new ItemStack[6]; for(int i=0;i<6;i++) out[i]=getItem(i); return out; }
    @Override public void setContents(ItemStack[] items) { for(int i=0;i<6;i++) setItem(i, items != null && i<items.length ? items[i] : null); }
    @Override public boolean contains(Material material) { return first(material)>=0; }
    @Override public int first(Material material) { if(material==null)return -1; for(int i=0;i<6;i++){ItemStack item=getItem(i); if(item!=null&&item.getType()==material)return i;} return -1; }
    @Override public void clear() { for(int i=0;i<6;i++)setItem(i,null); }
    @Override public void clear(int slot) { if(slot>=0&&slot<6)setItem(slot,null); }
}
