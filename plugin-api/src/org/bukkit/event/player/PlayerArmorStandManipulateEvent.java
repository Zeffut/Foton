package org.bukkit.event.player;

import org.bukkit.entity.ArmorStand;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.inventory.EquipmentSlot;
import org.bukkit.inventory.ItemStack;

/** Fired when a player manipulates an armor stand. */
public class PlayerArmorStandManipulateEvent extends PlayerEvent implements Cancellable {
    private final ArmorStand rightClicked; private final EquipmentSlot slot;
    private final ItemStack playerItem; private final ItemStack armorStandItem; private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public PlayerArmorStandManipulateEvent(Player player, ArmorStand rightClicked, EquipmentSlot slot, ItemStack playerItem) {
        super(player); this.rightClicked = rightClicked; this.slot = slot;
        this.playerItem = playerItem;
        this.armorStandItem = rightClicked == null ? null : itemAt(rightClicked, slot);
    }
    private static ItemStack itemAt(ArmorStand stand, EquipmentSlot slot) {
        if (slot == null) return null;
        return switch (slot) {
            case HAND -> stand.getEquipment().getItemInMainHand();
            case OFF_HAND -> stand.getEquipment().getItemInOffHand();
            case FEET -> stand.getEquipment().getArmorContents()[0];
            case LEGS -> stand.getEquipment().getArmorContents()[1];
            case CHEST -> stand.getEquipment().getArmorContents()[2];
            case HEAD -> stand.getEquipment().getArmorContents()[3];
            case BODY, SADDLE -> null;
        };
    }

    public ArmorStand getRightClicked() { return rightClicked; }
    public EquipmentSlot getSlot() { return slot; }
    public ItemStack getPlayerItem() { return playerItem; }
    public ItemStack getArmorStandItem() { return armorStandItem; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
