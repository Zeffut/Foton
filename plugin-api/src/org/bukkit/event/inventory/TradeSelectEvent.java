package org.bukkit.event.inventory;

import org.bukkit.entity.HumanEntity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.inventory.MerchantInventory;

/** Fired when a player selects a merchant offer. */
public class TradeSelectEvent extends InventoryEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final MerchantInventory inventory;
    private final int index;
    private boolean cancelled;

    public TradeSelectEvent(MerchantInventory inventory, int index) {
        super(null);
        this.inventory = inventory;
        this.index = index;
    }

    @Override public MerchantInventory getInventory() { return inventory; }
    public HumanEntity getWhoClicked() {
        java.util.List<HumanEntity> viewers = inventory == null ? java.util.List.of() : inventory.getViewers();
        return viewers.isEmpty() ? null : viewers.get(0);
    }
    public int getIndex() { return index; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
