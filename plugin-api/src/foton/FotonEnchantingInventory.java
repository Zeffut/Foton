package foton;

import org.bukkit.Location;
import org.bukkit.block.Block;
import org.bukkit.entity.HumanEntity;
import org.bukkit.event.inventory.InventoryType;
import org.bukkit.inventory.BlockInventoryHolder;
import org.bukkit.inventory.EnchantingInventory;
import org.bukkit.inventory.Inventory;
import org.bukkit.inventory.ItemStack;

/** The table's live slots, with a writeback snapshot during prepare dispatch. */
final class FotonEnchantingInventory extends FotonMenuInventory implements EnchantingInventory {
    private final FotonPlayer player;
    private final long instance;
    private final Block block;
    private ItemStack[] preparing;
    private final BlockInventoryHolder holder = new BlockInventoryHolder() {
        @Override public Block getBlock() { return block; }
        @Override public Inventory getInventory() { return FotonEnchantingInventory.this; }
    };

    FotonEnchantingInventory(FotonPlayer player, long instance, Block block, ItemStack[] preparing) {
        super(player.getUniqueId().toString());
        this.player = player;
        this.instance = instance;
        this.block = block;
        this.preparing = preparing;
    }

    void finishPrepare() { preparing = null; }
    @Override public int getSize() { return 2; }
    @Override public InventoryType getType() { return InventoryType.ENCHANTING; }
    @Override public BlockInventoryHolder getHolder() { return holder; }
    @Override public Location getLocation() { return block.getLocation(); }
    @Override public java.util.List<HumanEntity> getViewers() {
        if (preparing == null && Native.enchantmentTitle(player.getUniqueId().toString(), instance) == null)
            throw new IllegalStateException("The enchanting view is no longer open");
        return java.util.List.of(player);
    }
    @Override public ItemStack getItem(int slot) {
        if (slot < 0 || slot >= 2) throw new IndexOutOfBoundsException(slot);
        if (preparing != null) return preparing[slot];
        String encoded = Native.enchantmentItem(player.getUniqueId().toString(), instance, slot);
        if (encoded == null) throw new IllegalStateException("The enchanting view is no longer open");
        return FotonInventory.decode(encoded);
    }
    @Override public void setItem(int slot, ItemStack item) {
        if (slot < 0 || slot >= 2) throw new IndexOutOfBoundsException(slot);
        if (preparing != null) preparing[slot] = item == null ? null : item.clone();
        else if (!Native.setEnchantmentItem(player.getUniqueId().toString(), instance, slot, FotonInventory.encode(item)))
            throw new IllegalStateException("The enchanting view is no longer open");
    }
    @Override public ItemStack getItem() { return getItem(0); }
    @Override public void setItem(ItemStack item) { setItem(0, item); }
    @Override public ItemStack getSecondary() { return getItem(1); }
    @Override public void setSecondary(ItemStack item) { setItem(1, item); }
}
