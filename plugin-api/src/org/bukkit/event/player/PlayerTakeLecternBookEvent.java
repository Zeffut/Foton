package org.bukkit.event.player;

import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired before a player removes a book from a lectern. */
public final class PlayerTakeLecternBookEvent extends PlayerEvent implements Cancellable {
    private boolean cancelled;
    private final org.bukkit.block.Lectern lectern;
    private static final HandlerList HANDLERS = new HandlerList();
    public PlayerTakeLecternBookEvent(Player player) { this(player, null); }
    public PlayerTakeLecternBookEvent(Player player, org.bukkit.block.Lectern lectern) { super(player); this.lectern = lectern; }
    public org.bukkit.block.Lectern getLectern() { return lectern; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
