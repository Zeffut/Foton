package org.bukkit.event.player;

import org.bukkit.entity.Entity;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

public class PlayerInteractEntityEvent extends PlayerEvent implements Cancellable {
    private final Entity rightClicked;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();

    public PlayerInteractEntityEvent(Player player, Entity rightClicked) {
        super(player);
        this.rightClicked = rightClicked;
    }

    public Entity getRightClicked() { return rightClicked; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
