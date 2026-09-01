package foton;

import java.util.HashMap;
import org.bukkit.Material;
import org.bukkit.inventory.Inventory;
import org.bukkit.inventory.InventoryHolder;
import org.bukkit.inventory.ItemStack;

/** In-memory 27-slot inventory carried by a shulker item snapshot. */
final class FotonShulkerInventory implements Inventory {
    private final ItemStack[] slots = new ItemStack[27];
    @Override public InventoryHolder getHolder() { return null; }
    @Override public int getSize() { return slots.length; }
    @Override public ItemStack getItem(int slot) { return slot < 0 || slot >= slots.length ? null : slots[slot]; }
    @Override public void setItem(int slot, ItemStack item) { if (slot >= 0 && slot < slots.length) slots[slot] = item == null ? null : item.clone(); }
    @Override public HashMap<Integer, ItemStack> addItem(ItemStack... items) { HashMap<Integer, ItemStack> left = new HashMap<>(); if (items == null) return left; for (int i=0;i<items.length;i++) { ItemStack item=items[i]; boolean placed=false; for(int s=0;s<slots.length;s++) if(slots[s]==null || slots[s].getType().isAir()) { slots[s]=item==null?null:item.clone(); placed=true; break; } if(!placed && item!=null) left.put(i,item.clone()); } return left; }
    @Override public ItemStack[] getContents() { ItemStack[] copy = new ItemStack[slots.length]; for(int i=0;i<slots.length;i++) copy[i]=slots[i]==null?null:slots[i].clone(); return copy; }
    @Override public void setContents(ItemStack[] items) { for(int i=0;i<slots.length;i++) slots[i]=items!=null && i<items.length && items[i]!=null?items[i].clone():null; }
    @Override public boolean contains(Material material) { return first(material)>=0; }
    @Override public int first(Material material) { if(material==null)return -1; for(int i=0;i<slots.length;i++) if(slots[i]!=null&&slots[i].getType()==material)return i; return -1; }
    @Override public void clear() { java.util.Arrays.fill(slots,null); }
    @Override public void clear(int slot) { if(slot>=0&&slot<slots.length)slots[slot]=null; }
}
