package foton;

import org.bukkit.event.inventory.InventoryType;
import org.bukkit.inventory.GrindstoneInventory;
import org.bukkit.inventory.ItemStack;

public final class FotonGrindstoneInventory extends FotonMenuInventory implements GrindstoneInventory {
    FotonGrindstoneInventory(String owner) { super(owner); }
    FotonGrindstoneInventory(String owner, ItemStack[] snapshot) { super(owner, snapshot); }
    @Override public InventoryType getType() { return InventoryType.GRINDSTONE; }
    @Override public ItemStack getUpperItem() { return getItem(0); }
    @Override public void setUpperItem(ItemStack item) { setItem(0, item); }
    @Override public ItemStack getLowerItem() { return getItem(1); }
    @Override public void setLowerItem(ItemStack item) { setItem(1, item); }
    @Override public ItemStack getResult() { return getItem(2); }
    @Override public void setResult(ItemStack result) { setItem(2, result); }
}
