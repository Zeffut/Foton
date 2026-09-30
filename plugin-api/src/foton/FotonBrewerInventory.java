package foton;

import org.bukkit.block.BrewingStand;
import org.bukkit.inventory.BrewerInventory;
import org.bukkit.inventory.ItemStack;

/** Detached five-slot inventory used for BrewEvent's lock-free snapshot. */
public final class FotonBrewerInventory extends FotonMenuInventory implements BrewerInventory {
    private final BrewingStand holder;

    FotonBrewerInventory(BrewingStand holder, ItemStack[] contents) {
        super("", contents == null ? new ItemStack[5] : contents);
        this.holder = holder;
    }

    @Override public org.bukkit.event.inventory.InventoryType getType() {
        return org.bukkit.event.inventory.InventoryType.BREWING;
    }
    @Override public BrewingStand getHolder() { return holder; }
    @Override public ItemStack getIngredient() { return getItem(3); }
    @Override public void setIngredient(ItemStack ingredient) { setItem(3, ingredient); }
    @Override public ItemStack getFuel() { return getItem(4); }
    @Override public void setFuel(ItemStack fuel) { setItem(4, fuel); }
}
