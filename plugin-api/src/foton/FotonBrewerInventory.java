package foton;

import org.bukkit.inventory.BrewerInventory;
import org.bukkit.inventory.InventoryHolder;
import org.bukkit.inventory.ItemStack;

/** The live slots of a brewing stand: three bottles, ingredient, fuel.
 *
 * Foton has no {@code BrewingStand} block state yet, so there is no holder
 * to answer with. */
final class FotonBrewerInventory extends FotonHopperInventory implements BrewerInventory {
    FotonBrewerInventory(FotonBlockState stand) { super(stand, 5); }

    @Override public InventoryHolder getHolder() { return null; }
    @Override public org.bukkit.event.inventory.InventoryType getType() { return org.bukkit.event.inventory.InventoryType.BREWING; }
    @Override public ItemStack getIngredient() { return getItem(3); }
    @Override public void setIngredient(ItemStack ingredient) { setItem(3, ingredient); }
    @Override public ItemStack getFuel() { return getItem(4); }
    @Override public void setFuel(ItemStack fuel) { setItem(4, fuel); }
}
