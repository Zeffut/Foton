package org.bukkit.event.server;

import org.bukkit.Bukkit;
import org.bukkit.event.Event;

/** An event about the server rather than a player, entity or block. */
public abstract class ServerEvent extends Event {
    protected ServerEvent() {
        super(!Bukkit.isPrimaryThread());
    }

    protected ServerEvent(boolean asynchronous) {
        super(asynchronous);
    }
}
