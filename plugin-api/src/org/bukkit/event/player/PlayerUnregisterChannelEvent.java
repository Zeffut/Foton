package org.bukkit.event.player;

import org.bukkit.entity.Player;
import org.bukkit.event.HandlerList;

public class PlayerUnregisterChannelEvent extends PlayerEvent {
    private final String channel;
    private static final HandlerList HANDLERS = new HandlerList();

    public PlayerUnregisterChannelEvent(Player player, String channel) {
        super(player);
        this.channel = channel;
    }

    public String getChannel() {
        return channel;
    }

    @Override public HandlerList getHandlers() {
        return HANDLERS;
    }

    public static HandlerList getHandlerList() {
        return HANDLERS;
    }
}
