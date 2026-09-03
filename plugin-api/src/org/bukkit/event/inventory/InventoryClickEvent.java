package org.bukkit.event.inventory;

import org.bukkit.entity.HumanEntity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired before a player container click is applied. */
public class InventoryClickEvent extends InventoryEvent implements Cancellable {
    private final HumanEntity whoClicked;
    private org.bukkit.inventory.ItemStack currentItem;
    private org.bukkit.inventory.ItemStack cursor;
    private final ClickType click;
    private final int rawSlot;
    private final int hotbarButton;
    private boolean cancelled;
    private Event.Result result = Event.Result.DEFAULT;
    private static final HandlerList HANDLERS = new HandlerList();

    public InventoryClickEvent(HumanEntity whoClicked) { this(whoClicked, null); }
    public InventoryClickEvent(HumanEntity whoClicked, org.bukkit.inventory.ItemStack currentItem) { this(whoClicked, currentItem, ClickType.UNKNOWN, -1); }
    public InventoryClickEvent(HumanEntity whoClicked, org.bukkit.inventory.ItemStack currentItem, ClickType click) {
        this(whoClicked, currentItem, click, -1);
    }
    public InventoryClickEvent(HumanEntity whoClicked, org.bukkit.inventory.ItemStack currentItem, ClickType click, int rawSlot) {
        this(whoClicked, currentItem, null, click, rawSlot, -1);
    }
    public InventoryClickEvent(HumanEntity whoClicked, org.bukkit.inventory.ItemStack currentItem, org.bukkit.inventory.ItemStack cursor, ClickType click, int rawSlot) {
        this(whoClicked, currentItem, cursor, click, rawSlot, -1);
    }
    public InventoryClickEvent(HumanEntity whoClicked, org.bukkit.inventory.ItemStack currentItem, org.bukkit.inventory.ItemStack cursor, ClickType click, int rawSlot, int hotbarButton) {
        super(whoClicked instanceof org.bukkit.entity.Player ? ((org.bukkit.entity.Player) whoClicked).getOpenInventory() : null);
        this.whoClicked = whoClicked;
        this.currentItem = currentItem == null ? null : currentItem.clone();
        this.cursor = cursor == null ? null : cursor.clone();
        this.click = click == null ? ClickType.UNKNOWN : click;
        this.rawSlot = rawSlot;
        this.hotbarButton = hotbarButton;
    }
    public HumanEntity getWhoClicked() { return whoClicked; }
    public org.bukkit.inventory.ItemStack getCurrentItem() {
        return currentItem == null ? null : currentItem.clone();
    }
    public void setCurrentItem(org.bukkit.inventory.ItemStack item) {
        currentItem = item == null ? null : item.clone();
    }
    public ClickType getClick() { return click; }
    public org.bukkit.inventory.ItemStack getCursor() { return cursor == null ? null : cursor.clone(); }
    /** Changes the cursor stack that will be applied after the event. */
    public void setCursor(org.bukkit.inventory.ItemStack item) {
        cursor = item == null ? null : item.clone();
    }
    public int getRawSlot() { return rawSlot; }
    public int getHotbarButton() { return hotbarButton; }
    public int getSlot() { return rawSlot; }
    public InventoryAction getAction() {
        switch (click) {
            case LEFT: return InventoryAction.PICKUP_ALL;
            case RIGHT: return InventoryAction.PICKUP_HALF;
            case SHIFT_LEFT: case SHIFT_RIGHT: return InventoryAction.MOVE_TO_OTHER_INVENTORY;
            case DROP: return InventoryAction.DROP_ALL_SLOT;
            case CONTROL_DROP: return InventoryAction.DROP_ONE_SLOT;
            case MIDDLE: return InventoryAction.CLONE_STACK;
            case DOUBLE_CLICK: return InventoryAction.COLLECT_TO_CURSOR;
            default: return InventoryAction.UNKNOWN;
        }
    }
    public InventoryType.SlotType getSlotType() {
        return rawSlot < 0 ? InventoryType.SlotType.OUTSIDE : InventoryType.SlotType.CONTAINER;
    }
    public org.bukkit.inventory.Inventory getClickedInventory() {
        org.bukkit.inventory.InventoryView view = getView();
        if (view == null || rawSlot < 0) return null;
        return rawSlot < view.getTopInventory().getSize() ? view.getTopInventory() : view.getBottomInventory();
    }
    public org.bukkit.inventory.Inventory getInventory() {
        org.bukkit.inventory.InventoryView view = getView();
        return view == null ? null : view.getTopInventory();
    }
    public Event.Result getResult() { return result; }
    public void setResult(Event.Result result) { this.result = result == null ? Event.Result.DEFAULT : result; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
