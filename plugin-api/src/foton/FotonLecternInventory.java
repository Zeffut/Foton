package foton;

import java.util.HashMap;
import org.bukkit.Material;
import org.bukkit.inventory.Inventory;
import org.bukkit.inventory.InventoryHolder;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.meta.BookMeta;

/** The one real slot exposed by a lectern block entity. */
final class FotonLecternInventory implements Inventory {
    private final FotonBlock block;

    FotonLecternInventory(FotonBlock block) { this.block = block; }

    @Override public InventoryHolder getHolder() { return (FotonLectern) block.getState(); }
    @Override public int getSize() { return 1; }

    @Override public ItemStack getItem(int slot) {
        if (slot != 0 || block.getWorld() == null) return null;
        String encoded = Native.lecternBook(block.getWorld().getName(), block.getX(), block.getY(), block.getZ());
        ItemStack item = FotonInventory.decode(encoded);
        if (item == null || (item.getType() != Material.WRITTEN_BOOK && item.getType() != Material.WRITABLE_BOOK)) return item;
        String[] pages = Native.lecternBookPages(block.getWorld().getName(), block.getX(), block.getY(), block.getZ());
        BookMeta meta = (BookMeta) item.getItemMeta();
        meta.setPages(pages == null ? new String[0] : pages);
        item.setItemMeta(meta);
        return item;
    }

    @Override public void setItem(int slot, ItemStack item) {
        if (slot != 0 || block.getWorld() == null) return;
        String world = block.getWorld().getName();
        if (item == null || item.getType().isAir()) {
            Native.lecternClearBook(world, block.getX(), block.getY(), block.getZ());
            return;
        }
        Native.lecternSetBook(world, block.getX(), block.getY(), block.getZ(), FotonInventory.encode(item));
    }

    @Override
    public HashMap<Integer, ItemStack> addItem(ItemStack... items) {
        HashMap<Integer, ItemStack> leftovers = new HashMap<>();
        if (items == null) return leftovers;
        for (int index = 0; index < items.length; index++) {
            ItemStack item = items[index];
            if (item == null || item.getType().isAir()) continue;
            boolean book = item.getType() == Material.WRITTEN_BOOK
                || item.getType() == Material.WRITABLE_BOOK;
            if (!book || getItem(0) != null) {
                leftovers.put(index, item.clone());
                continue;
            }
            setItem(0, item);
        }
        return leftovers;
    }
    @Override public ItemStack[] getContents() { return new ItemStack[] { getItem(0) }; }
    @Override public void setContents(ItemStack[] items) { if (items != null && items.length > 0) setItem(0, items[0]); }
    @Override public boolean contains(Material material) { ItemStack item = getItem(0); return item != null && item.getType() == material; }
    @Override public int first(Material material) { return contains(material) ? 0 : -1; }
    @Override public void clear() { clear(0); }
    @Override public void clear(int slot) {
        if (slot == 0 && block.getWorld() != null) {
            Native.lecternClearBook(block.getWorld().getName(), block.getX(), block.getY(), block.getZ());
        }
    }
}
