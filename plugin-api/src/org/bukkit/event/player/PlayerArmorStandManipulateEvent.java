package org.bukkit.event.player;

import org.bukkit.entity.ArmorStand;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.inventory.EquipmentSlot;
import org.bukkit.inventory.ItemStack;

/** Fired when a player trades an item with an armor stand. */
public class PlayerArmorStandManipulateEvent extends PlayerEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final ArmorStand rightClicked;
    private final ItemStack playerItem;
    private final ItemStack armorStandItem;
    private final EquipmentSlot slot;
    private final EquipmentSlot hand;
    private boolean cancelled;

    public PlayerArmorStandManipulateEvent(Player player, ArmorStand rightClicked, ItemStack playerItem,
            ItemStack armorStandItem, EquipmentSlot slot, EquipmentSlot hand) {
        super(player);
        this.rightClicked = rightClicked;
        this.playerItem = playerItem;
        this.armorStandItem = armorStandItem;
        this.slot = slot;
        this.hand = hand;
    }

    public ArmorStand getRightClicked() { return rightClicked; }
    /** The item the player holds. */
    public ItemStack getPlayerItem() { return playerItem; }
    /** The item the stand wears in {@link #getSlot()}. */
    public ItemStack getArmorStandItem() { return armorStandItem; }
    /** The stand's slot the swap is about. */
    public EquipmentSlot getSlot() { return slot; }
    /** The hand the player used. */
    public EquipmentSlot getHand() { return hand; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
