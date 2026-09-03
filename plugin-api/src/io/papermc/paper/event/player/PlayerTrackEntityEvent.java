package io.papermc.paper.event.player;

import org.bukkit.entity.Entity;
import org.bukkit.entity.Player;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired when an entity becomes tracked by a player. */
public final class PlayerTrackEntityEvent extends Event {
    private final Player player;
    private final Entity entity;
    private static final HandlerList HANDLERS = new HandlerList();
    public PlayerTrackEntityEvent(Player player, Entity entity) { this.player = player; this.entity = entity; }
    public Player getPlayer() { return player; }
    public Entity getEntity() { return entity; }
    public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
