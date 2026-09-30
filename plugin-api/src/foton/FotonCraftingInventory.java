package foton;

import org.bukkit.event.inventory.InventoryType;
import org.bukkit.inventory.CraftingInventory;
import org.bukkit.inventory.ItemStack;

/** A crafting grid and its result, slot 0 the result: a table's 3x3, or the
 * 2x2 in the player's own inventory. */
public final class FotonCraftingInventory extends FotonMenuInventory implements CraftingInventory {
    FotonCraftingInventory(String owner) { super(owner); }

    /** A copy, for a listener that runs while the menu itself is busy. */
    FotonCraftingInventory(String owner, ItemStack[] slots) { super(owner, slots); }

    @Override public InventoryType getType() {
        return getSize() == 5 ? InventoryType.CRAFTING : InventoryType.WORKBENCH;
    }

    private int gridSize() { return Math.max(0, getSize() - 1); }

    @Override public ItemStack[] getMatrix() {
        ItemStack[] result = new ItemStack[gridSize()];
        for (int i = 0; i < result.length; i++) result[i] = getItem(i + 1);
        return result;
    }
    @Override public void setMatrix(ItemStack[] matrix) {
        for (int i = 0; i < gridSize(); i++) setItem(i + 1, matrix != null && i < matrix.length ? matrix[i] : null);
    }
    @Override public ItemStack getResult() { return getItem(0); }
    @Override public void setResult(ItemStack result) { setItem(0, result); }

    /** The recipe the grid makes, as Paper's {@code getRecipe} answers it, or null. */
    public org.bukkit.inventory.Recipe recipe() {
        ItemStack[] matrix = getMatrix();
        int width = matrix.length == 4 ? 2 : 3;
        StringBuilder grid = new StringBuilder();
        for (int i = 0; i < matrix.length; i++) {
            if (i > 0) grid.append(EventRelay.ITEM);
            grid.append(FotonInventory.encode(matrix[i]));
        }
        String key = Native.craftingRecipe(grid.toString(), width);
        org.bukkit.NamespacedKey recipe = key == null ? null : org.bukkit.NamespacedKey.fromString(key);
        return recipe == null ? null : new FotonCraftingRecipe(recipe, getResult());
    }
}
