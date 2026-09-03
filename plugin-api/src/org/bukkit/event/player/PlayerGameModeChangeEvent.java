package org.bukkit.event.player;

import org.bukkit.GameMode;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired before a player's game mode changes. */
public class PlayerGameModeChangeEvent extends PlayerEvent implements Cancellable {
    private final GameMode newGameMode;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public PlayerGameModeChangeEvent(Player player, GameMode newGameMode) { super(player); this.newGameMode = newGameMode; }
    public GameMode getNewGameMode() { return newGameMode; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
