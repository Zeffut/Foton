package org.bukkit.event.player;

import org.bukkit.entity.Entity;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired when a fishing hook is retrieved. */
public class PlayerFishEvent extends PlayerEvent implements Cancellable {
    public enum State { FISHING, CAUGHT_FISH, CAUGHT_ENTITY, IN_GROUND, FAILED_ATTEMPT, REEL_IN, BITE, LURED, HOOKED_ENTITY }
    private final Entity hook;
    private final Entity caught;
    private final State state;
    private boolean cancelled;
    private int expToDrop;
    private org.bukkit.inventory.EquipmentSlot hand = org.bukkit.inventory.EquipmentSlot.HAND;
    private static final HandlerList HANDLERS = new HandlerList();

    public PlayerFishEvent(Player player, Entity hook, Entity caught, State state) {
        super(player); this.hook = hook; this.caught = caught; this.state = state;
    }
    public PlayerFishEvent(Player player, Entity hook, State state) { this(player, hook, null, state); }
    public org.bukkit.entity.FishHook getHook() { return hook instanceof org.bukkit.entity.FishHook ? (org.bukkit.entity.FishHook) hook : null; }
    public Entity getCaught() { return caught; }
    public State getState() { return state; }
    public org.bukkit.inventory.EquipmentSlot getHand() { return hand; }
    public void setHand(org.bukkit.inventory.EquipmentSlot value) { if (value != null) hand = value; }
    public int getExpToDrop() { return expToDrop; }
    public void setExpToDrop(int value) { expToDrop = Math.max(0, value); }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
