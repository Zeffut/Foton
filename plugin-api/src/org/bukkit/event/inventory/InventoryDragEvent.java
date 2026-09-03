package org.bukkit.event.inventory;

import java.util.Collections;
import java.util.LinkedHashSet;
import java.util.Set;
import org.bukkit.entity.HumanEntity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired when a player completes an inventory drag before distribution. */
public class InventoryDragEvent extends Event implements Cancellable {
    private final HumanEntity who; private final Set<Integer> rawSlots; private final org.bukkit.inventory.ItemStack oldCursor;
    private final DragType type; private final java.util.Map<Integer, org.bukkit.inventory.ItemStack> newItems; private boolean cancelled; private static final HandlerList HANDLERS = new HandlerList();
    public InventoryDragEvent(HumanEntity who, Set<Integer> rawSlots, org.bukkit.inventory.ItemStack oldCursor, DragType type) {
        this.who = who; this.rawSlots = rawSlots == null ? Collections.emptySet() : new LinkedHashSet<>(rawSlots);
        this.oldCursor = oldCursor == null ? null : oldCursor.clone(); this.type = type == null ? DragType.EVEN : type;
        java.util.Map<Integer, org.bukkit.inventory.ItemStack> computed = new java.util.LinkedHashMap<>();
        if (this.oldCursor != null && !this.rawSlots.isEmpty()) {
            int total = this.oldCursor.getAmount();
            int each = this.type == DragType.SINGLE ? 1 : Math.max(1, total / this.rawSlots.size());
            int remaining = total;
            for (Integer slot : this.rawSlots) {
                if (remaining <= 0) break;
                org.bukkit.inventory.ItemStack stack = this.oldCursor.clone();
                stack.setAmount(Math.min(each, remaining));
                computed.put(slot, stack);
                remaining -= stack.getAmount();
            }
        }
        this.newItems = java.util.Collections.unmodifiableMap(computed);
    }
    public HumanEntity getWhoClicked() { return who; }
    public org.bukkit.inventory.InventoryView getView() {
        return who instanceof org.bukkit.entity.Player ? ((org.bukkit.entity.Player) who).getOpenInventory() : null;
    }
    public org.bukkit.inventory.Inventory getInventory() {
        org.bukkit.inventory.InventoryView view = getView();
        return view == null ? null : view.getTopInventory();
    }
    public Set<Integer> getRawSlots() { return Collections.unmodifiableSet(rawSlots); }
    public org.bukkit.inventory.ItemStack getOldCursor() { return oldCursor == null ? null : oldCursor.clone(); }
    public DragType getType() { return type; }
    public java.util.Map<Integer, org.bukkit.inventory.ItemStack> getNewItems() {
        java.util.Map<Integer, org.bukkit.inventory.ItemStack> copy = new java.util.LinkedHashMap<>();
        for (java.util.Map.Entry<Integer, org.bukkit.inventory.ItemStack> entry : newItems.entrySet()) copy.put(entry.getKey(), entry.getValue().clone());
        return java.util.Collections.unmodifiableMap(copy);
    }
    public boolean isCancelled() { return cancelled; } public void setCancelled(boolean value) { cancelled = value; }
    public HandlerList getHandlers() { return HANDLERS; } public static HandlerList getHandlerList() { return HANDLERS; }
    public enum DragType { SINGLE, EVEN }
}
