package com.destroystokyo.paper.event.inventory;

import org.bukkit.event.inventory.PrepareInventoryResultEvent;
import org.bukkit.inventory.InventoryView;
import org.bukkit.inventory.ItemStack;

/** Paper's name for a workstation screen working out its result. */
@SuppressWarnings("deprecation")
public class PrepareResultEvent extends PrepareInventoryResultEvent {
    public PrepareResultEvent(InventoryView inventory, ItemStack result) {
        super(inventory, result);
    }

    @Override public ItemStack getResult() { return super.getResult(); }
    @Override public void setResult(ItemStack result) { super.setResult(result); }
}
