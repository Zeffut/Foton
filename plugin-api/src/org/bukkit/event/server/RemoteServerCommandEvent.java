package org.bukkit.event.server;

import org.bukkit.command.CommandSender;

/** Fired when a remote console issues a command. */
public class RemoteServerCommandEvent extends ServerCommandEvent {
    public RemoteServerCommandEvent(CommandSender sender, String command) {
        super(sender, command);
    }
}
