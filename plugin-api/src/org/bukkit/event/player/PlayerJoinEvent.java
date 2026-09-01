package org.bukkit.event.player;

import org.bukkit.entity.Player;

public class PlayerJoinEvent extends PlayerEvent {
    private String joinMessage;

    public PlayerJoinEvent(Player player, String joinMessage) {
        super(player);
        this.joinMessage = joinMessage;
    }

    public String getJoinMessage() { return joinMessage; }

    /** Sets what is announced. Null or empty announces nothing. */
    public void setJoinMessage(String message) { this.joinMessage = message; }
}
