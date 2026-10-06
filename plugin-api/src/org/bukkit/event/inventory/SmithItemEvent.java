package org.bukkit.event.inventory;

import org.bukkit.entity.HumanEntity;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.SmithingInventory;

/** A click on a smithing table's result: the click that forges. Listeners of
 * {@link InventoryClickEvent} receive it too. */
public class SmithItemEvent extends InventoryClickEvent {
    public SmithItemEvent(HumanEntity whoClicked, ItemStack currentItem, ItemStack cursor, ClickType click,
            int rawSlot) {
        super(whoClicked, currentItem, cursor, click, rawSlot);
    }
    public SmithItemEvent(HumanEntity whoClicked, ItemStack currentItem, ItemStack cursor, ClickType click,
            int rawSlot, int hotbarButton) {
        super(whoClicked, currentItem, cursor, click, rawSlot, hotbarButton);
    }

    @Override public SmithingInventory getInventory() { return (SmithingInventory) super.getInventory(); }
}
