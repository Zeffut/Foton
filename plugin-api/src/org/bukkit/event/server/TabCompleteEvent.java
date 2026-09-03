package org.bukkit.event.server;

import java.util.ArrayList;
import java.util.List;
import org.bukkit.Location;
import org.bukkit.command.CommandSender;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired when command or chat tab completion is requested. */
public class TabCompleteEvent extends Event implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final CommandSender sender;
    private final String buffer;
    private final boolean command;
    private final Location location;
    private List<String> completions;
    private boolean cancelled;

    public TabCompleteEvent(CommandSender sender, String buffer, List<String> completions) {
        this(sender, buffer, completions, buffer != null && buffer.startsWith("/"), null);
    }

    public TabCompleteEvent(CommandSender sender, String buffer, List<String> completions,
            boolean isCommand, Location location) {
        super(!org.bukkit.Bukkit.isPrimaryThread());
        this.sender = sender;
        this.buffer = buffer == null ? "" : buffer;
        this.completions = completions == null ? new ArrayList<>() : new ArrayList<>(completions);
        this.command = isCommand;
        this.location = location;
    }

    public CommandSender getSender() { return sender; }
    public String getBuffer() { return buffer; }
    public List<String> getCompletions() { return completions; }
    public void setCompletions(List<String> completions) {
        this.completions = completions == null ? new ArrayList<>() : new ArrayList<>(completions);
    }
    public boolean isCommand() { return command; }
    public Location getLocation() { return location; }
    public boolean isCancelled() { return cancelled; }
    public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
