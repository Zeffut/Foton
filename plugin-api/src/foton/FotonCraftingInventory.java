package foton;

import org.bukkit.event.inventory.InventoryType;
import org.bukkit.inventory.CraftingInventory;
import org.bukkit.inventory.ItemStack;

public final class FotonCraftingInventory extends FotonMenuInventory implements CraftingInventory {
    FotonCraftingInventory(String owner) { super(owner); }
    @Override public InventoryType getType() { return InventoryType.WORKBENCH; }
    @Override public ItemStack[] getMatrix() {
        ItemStack[] result = new ItemStack[9];
        for (int i = 0; i < result.length; i++) result[i] = getItem(i + 1);
        return result;
    }
    @Override public void setMatrix(ItemStack[] matrix) {
        for (int i = 0; i < 9; i++) setItem(i + 1, matrix != null && i < matrix.length ? matrix[i] : null);
    }
    @Override public ItemStack getResult() { return getItem(0); }
    @Override public void setResult(ItemStack result) { setItem(0, result); }
}
