package org.bukkit.event.player;

import org.bukkit.entity.Player;
import org.bukkit.event.Event;

public abstract class PlayerEvent extends Event {
    protected final Player player;

    protected PlayerEvent(Player player) { this.player = player; }

    public final Player getPlayer() { return player; }
}
