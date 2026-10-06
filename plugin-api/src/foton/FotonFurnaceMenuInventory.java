package foton;

import org.bukkit.Material;
import org.bukkit.block.Furnace;
import org.bukkit.inventory.FurnaceInventory;
import org.bukkit.inventory.ItemStack;

/** The three furnace slots of a player's open furnace, smoker or blast
 * furnace screen.
 *
 * {@link #getHolder()} is the furnace the screen was opened on, as in Bukkit. */
final class FotonFurnaceMenuInventory extends FotonMenuInventory implements FurnaceInventory {
    private final Material block;

    FotonFurnaceMenuInventory(String owner, Material block) {
        super(owner);
        this.block = block;
    }

    static boolean fuel(ItemStack item) {
        return item != null && Native.isFuel(FotonInventory.encode(item));
    }

    static boolean smeltable(Material block, ItemStack item) {
        return item != null && Native.cookingRecipe(block.getKey().toString(), FotonInventory.encode(item)) != null;
    }

    @Override public org.bukkit.event.inventory.InventoryType getType() {
        return switch (block) {
            case BLAST_FURNACE -> org.bukkit.event.inventory.InventoryType.BLAST_FURNACE;
            case SMOKER -> org.bukkit.event.inventory.InventoryType.SMOKER;
            default -> org.bukkit.event.inventory.InventoryType.FURNACE;
        };
    }
    @Override public ItemStack getSmelting() { return getItem(0); }
    @Override public void setSmelting(ItemStack stack) { setItem(0, stack); }
    @Override public ItemStack getFuel() { return getItem(1); }
    @Override public void setFuel(ItemStack stack) { setItem(1, stack); }
    @Override public ItemStack getResult() { return getItem(2); }
    @Override public void setResult(ItemStack stack) { setItem(2, stack); }
    @Override public boolean isFuel(ItemStack item) { return fuel(item); }
    @Override public boolean isSmeltable(ItemStack item) { return smeltable(block, item); }
    @Override public Furnace getHolder() { return FotonMenuSource.holder(owner()) instanceof Furnace furnace ? furnace : null; }
}
