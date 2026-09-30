package foton;

import org.bukkit.event.inventory.InventoryType;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.Recipe;
import org.bukkit.inventory.SmithingInventory;

/** A smithing table's slots: template, base, addition, result.
 *
 * Foton does not hand smithing recipes to plugins yet, so
 * {@link #getRecipe()} has none to answer with. */
final class FotonSmithingInventory extends FotonMenuInventory implements SmithingInventory {
    FotonSmithingInventory(String owner) { super(owner); }
    FotonSmithingInventory(String owner, ItemStack[] snapshot) { super(owner, snapshot); }
    @Override public InventoryType getType() { return InventoryType.SMITHING; }
    @Override public ItemStack getResult() { return getItem(3); }
    @Override public void setResult(ItemStack result) { setItem(3, result); }
    @Override public Recipe getRecipe() { return null; }
}
