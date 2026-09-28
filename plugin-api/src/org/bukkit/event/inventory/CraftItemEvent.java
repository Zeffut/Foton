package org.bukkit.event.inventory;

import org.bukkit.entity.HumanEntity;
import org.bukkit.inventory.CraftingInventory;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.Recipe;

/** A click on a crafting table's result: the click that crafts. Listeners of
 * {@link InventoryClickEvent} receive it too. */
public class CraftItemEvent extends InventoryClickEvent {
    private final Recipe recipe;
    public CraftItemEvent(HumanEntity whoClicked, Recipe recipe) { super(whoClicked); this.recipe = recipe; }
    public CraftItemEvent(Recipe recipe, HumanEntity whoClicked, ItemStack currentItem, ItemStack cursor,
            ClickType click, int rawSlot) {
        super(whoClicked, currentItem, cursor, click, rawSlot);
        this.recipe = recipe;
    }
    /** A craft whose grid the host copied into {@code view}'s top inventory. */
    public CraftItemEvent(Recipe recipe, org.bukkit.inventory.InventoryView view, HumanEntity whoClicked,
            ItemStack currentItem, ItemStack cursor, ClickType click, int rawSlot) {
        super(view, whoClicked, currentItem, cursor, click, rawSlot, -1);
        this.recipe = recipe;
    }
    public Recipe getRecipe() { return recipe; }
    @Override public InventoryType.SlotType getSlotType() { return InventoryType.SlotType.RESULT; }
    @Override public CraftingInventory getInventory() { return (CraftingInventory) super.getInventory(); }
}
