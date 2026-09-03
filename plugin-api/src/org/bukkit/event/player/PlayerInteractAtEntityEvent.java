package org.bukkit.event.player;

import org.bukkit.entity.Entity;
import org.bukkit.entity.Player;
import org.bukkit.event.HandlerList;
import org.bukkit.util.Vector;

/** Interaction event carrying the exact hit position on an entity. */
public class PlayerInteractAtEntityEvent extends PlayerInteractEntityEvent {
    private final Vector clickedPosition;
    private static final HandlerList HANDLERS = new HandlerList();

    public PlayerInteractAtEntityEvent(Player player, Entity rightClicked, Vector clickedPosition) {
        super(player, rightClicked);
        this.clickedPosition = clickedPosition == null ? new Vector() : clickedPosition.clone();
    }

    public Vector getClickedPosition() { return clickedPosition.clone(); }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
