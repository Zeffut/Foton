package org.bukkit.event.player;

import org.bukkit.entity.Egg;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired when a player's thrown egg hatches. */
public class PlayerEggThrowEvent extends Event implements Cancellable {
    private final Player player;
    private final Egg egg;
    private boolean hatching = true;
    private byte numHatches;
    private org.bukkit.entity.EntityType hatchType = org.bukkit.entity.EntityType.CHICKEN;
    private static final HandlerList HANDLERS = new HandlerList();
    public PlayerEggThrowEvent(Player player, Egg egg) { this.player = player; this.egg = egg; }
    public Player getPlayer() { return player; }
    public Egg getEgg() { return egg; }
    public boolean isHatching() { return hatching; }
    public void setHatching(boolean hatching) { this.hatching = hatching; }
    public byte getNumHatches() { return numHatches; }
    public void setNumHatches(byte numHatches) { this.numHatches = numHatches; }
    public org.bukkit.entity.EntityType getHatchType() { return hatchType; }
    public void setHatchType(org.bukkit.entity.EntityType type) { if (type != null) hatchType = type; }
    @Override public boolean isCancelled() { return !hatching; }
    @Override public void setCancelled(boolean value) { hatching = !value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
