package org.bukkit.event.entity;

import org.bukkit.entity.Entity;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;
import org.bukkit.inventory.EquipmentSlot;

/** Called when a player leashes an entity. */
public class PlayerLeashEntityEvent extends Event implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final Entity entity, leashHolder;
    private final Player player;
    private final EquipmentSlot hand;
    private boolean cancelled;
    public PlayerLeashEntityEvent(Entity entity, Entity leashHolder, Player leasher) {
        this(entity, leashHolder, leasher, EquipmentSlot.HAND);
    }
    public PlayerLeashEntityEvent(Entity entity, Entity leashHolder, Player leasher, EquipmentSlot hand) {
        this.entity = entity; this.leashHolder = leashHolder; this.player = leasher; this.hand = hand;
    }
    public Entity getLeashHolder() { return leashHolder; }
    public Entity getEntity() { return entity; }
    public Player getPlayer() { return player; }
    public EquipmentSlot getHand() { return hand; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancel) { cancelled = cancel; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
