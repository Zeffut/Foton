package org.bukkit.event.player;

import org.bukkit.entity.Player;

public class PlayerRegisterChannelEvent extends PlayerEvent {
    private final String channel;

    public PlayerRegisterChannelEvent(Player player, String channel) {
        super(player);
        this.channel = channel;
    }

    public String getChannel() { return channel; }
}
