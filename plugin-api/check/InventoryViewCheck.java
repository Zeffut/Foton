import java.util.HashMap;
import org.bukkit.Material;
import org.bukkit.inventory.Inventory;
import org.bukkit.inventory.InventoryHolder;
import org.bukkit.inventory.InventoryView;
import org.bukkit.inventory.ItemStack;

public final class InventoryViewCheck {
    private static final class SizedInventory implements Inventory {
        private final int size;
        SizedInventory(int size) { this.size = size; }
        public InventoryHolder getHolder() { return null; }
        public int getSize() { return size; }
        public ItemStack getItem(int slot) { return null; }
        public void setItem(int slot, ItemStack item) {}
        public HashMap<Integer, ItemStack> addItem(ItemStack... items) { return new HashMap<>(); }
        public ItemStack[] getContents() { return new ItemStack[size]; }
        public void setContents(ItemStack[] items) {}
        public boolean contains(Material material) { return false; }
        public int first(Material material) { return -1; }
        public void clear() {}
        public void clear(int slot) {}
    }

    private static InventoryView view(int top, int bottom) {
        Inventory upper = new SizedInventory(top);
        Inventory lower = new SizedInventory(bottom);
        return new InventoryView() {
            public Inventory getTopInventory() { return upper; }
            public Inventory getBottomInventory() { return lower; }
            public org.bukkit.entity.HumanEntity getPlayer() { return null; }
            public String getTitle() { return ""; }
        };
    }

    public static void check() {
        InventoryView chest = view(27, 41);
        Checks.expect(chest.convertSlot(0) == 0, "top raw slot 0 must stay slot 0");
        Checks.expect(chest.convertSlot(26) == 26, "last top slot must stay unchanged");
        Checks.expect(chest.convertSlot(27) == 9, "first visible player slot must map to main slot 9");
        Checks.expect(chest.convertSlot(53) == 35, "last visible main slot must map to slot 35");
        Checks.expect(chest.convertSlot(54) == 0, "first hotbar raw slot must map to slot 0");
        Checks.expect(chest.convertSlot(62) == 8, "last hotbar raw slot must map to slot 8");
        Checks.expect(chest.convertSlot(63) == -1, "raw slots beyond the visible player inventory are invalid");

        InventoryView player = view(5, 41);
        Checks.expect(player.convertSlot(5) == 9, "player inventory bottom must use the same display mapping");
        Checks.expect(player.convertSlot(40) == 8, "player inventory hotbar must map after main storage");
    }
}
