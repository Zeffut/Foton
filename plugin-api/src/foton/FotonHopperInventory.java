package foton;

import java.util.HashMap;
import org.bukkit.Material;
import org.bukkit.block.Hopper;
import org.bukkit.inventory.Inventory;
import org.bukkit.inventory.InventoryHolder;
import org.bukkit.inventory.ItemStack;

/** Five-slot live view of a vanilla hopper block entity. */
class FotonHopperInventory implements Inventory {
    private final FotonBlockState holder;
    private final int size;
    FotonHopperInventory(FotonBlockState holder, int size) { this.holder = holder; this.size = size; }
    private String world() { return holder.getWorld().getName(); }
    @Override public InventoryHolder getHolder() { return (InventoryHolder) holder; }
    @Override public int getSize() { return size; }
    @Override public ItemStack getItem(int slot) { return FotonInventory.decode(Native.hopperInventorySlot(world(), holder.getX(), holder.getY(), holder.getZ(), slot)); }
    @Override public void setItem(int slot, ItemStack item) { if (slot >= 0 && slot < size) Native.hopperSetInventorySlot(world(), holder.getX(), holder.getY(), holder.getZ(), slot, FotonInventory.encode(item)); }
    @Override public HashMap<Integer, ItemStack> addItem(ItemStack... items) {
        HashMap<Integer, ItemStack> leftovers = new HashMap<>();
        if (items == null) return leftovers;
        for (int index = 0; index < items.length; index++) {
            ItemStack incoming = items[index] == null ? null : items[index].clone();
            if (incoming == null || incoming.getType().isAir() || incoming.getAmount() <= 0) continue;
            for (int slot = 0; slot < size && incoming.getAmount() > 0; slot++) {
                ItemStack current = getItem(slot);
                if (current == null || !current.isSimilar(incoming)) continue;
                int space = Math.max(0, current.getMaxStackSize() - current.getAmount());
                int moved = Math.min(space, incoming.getAmount());
                if (moved > 0) {
                    current.setAmount(current.getAmount() + moved);
                    incoming.setAmount(incoming.getAmount() - moved);
                    setItem(slot, current);
                }
            }
            for (int slot = 0; slot < size && incoming.getAmount() > 0; slot++) {
                ItemStack current = getItem(slot);
                if (current != null && !current.getType().isAir() && current.getAmount() > 0) continue;
                int moved = Math.min(incoming.getMaxStackSize(), incoming.getAmount());
                ItemStack placed = incoming.clone();
                placed.setAmount(moved);
                setItem(slot, placed);
                incoming.setAmount(incoming.getAmount() - moved);
            }
            if (incoming.getAmount() > 0) leftovers.put(index, incoming);
        }
        return leftovers;
    }

    @Override public ItemStack[] getContents() { ItemStack[] out=new ItemStack[size]; for(int i=0;i<size;i++) out[i]=getItem(i); return out; }
    @Override public void setContents(ItemStack[] items) { for(int i=0;i<size;i++) setItem(i, items != null && i<items.length ? items[i] : null); }
    @Override public boolean contains(Material material) { return first(material)>=0; }
    @Override public int first(Material material) { if(material==null)return -1; for(int i=0;i<size;i++){ItemStack item=getItem(i); if(item!=null&&item.getType()==material)return i;} return -1; }
    @Override public void clear() { for(int i=0;i<size;i++)setItem(i,null); }
    @Override public void clear(int slot) { if(slot>=0&&slot<size)setItem(slot,null); }
}
