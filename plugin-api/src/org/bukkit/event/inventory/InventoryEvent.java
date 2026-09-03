package org.bukkit.event.inventory;

import org.bukkit.event.Event;

/** Base for inventory events carrying the view affected by the event. */
public abstract class InventoryEvent extends Event {
    private final org.bukkit.inventory.InventoryView view;

    protected InventoryEvent(org.bukkit.inventory.InventoryView view) {
        this.view = view;
    }

    public org.bukkit.inventory.InventoryView getView() {
        return view;
    }

    public org.bukkit.inventory.Inventory getInventory() {
        return view == null ? null : view.getTopInventory();
    }
}
