package org.bukkit.event.server;

import org.bukkit.command.CommandSender;
import org.bukkit.event.HandlerList;

/** Fired when the server dispatches a command. */
public class ServerCommandEvent extends ServerEvent {
    private final CommandSender sender;
    private final String command;
    private static final HandlerList HANDLERS = new HandlerList();

    public ServerCommandEvent(CommandSender sender, String command) {
        this.sender = sender;
        this.command = command == null ? "" : command;
    }

    public CommandSender getSender() { return sender; }
    public String getCommand() { return command; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
