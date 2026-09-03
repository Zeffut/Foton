package com.destroystokyo.paper.event.server;

import java.util.ArrayList;
import java.util.List;
import org.bukkit.command.CommandSender;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Paper-compatible tab completion event. */
public class AsyncTabCompleteEvent extends Event implements Cancellable {
    public static final class Completion {
        private final String completion;
        private Completion(String completion) { this.completion = completion == null ? "" : completion; }
        public static Completion completion(String value) { return new Completion(value); }
        public String getCompletion() { return completion; }
    }
    private final CommandSender sender;
    private final String buffer;
    private List<String> completions = new ArrayList<>();
    private boolean handled;
    private final boolean command;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public AsyncTabCompleteEvent(CommandSender sender, String buffer, List<String> completions) { this(sender, buffer, completions, buffer != null && buffer.startsWith("/")); }
    public AsyncTabCompleteEvent(CommandSender sender, String buffer, List<String> completions, boolean isCommand) {
        this.sender = sender; this.buffer = buffer == null ? "" : buffer; this.command = isCommand;
        if (completions != null) this.completions = new ArrayList<>(completions);
    }
    public CommandSender getSender() { return sender; }
    public String getBuffer() { return buffer; }
    public List<String> getCompletions() { return completions; }
    public void setCompletions(List<String> value) { completions = value == null ? new ArrayList<>() : new ArrayList<>(value); }
    /** Paper's fluent-style setter used by newer tab completion handlers. */
    public void completions(List<String> value) { setCompletions(value); }
    public boolean isCommand() { return command; }
    public boolean isHandled() { return handled; }
    public void setHandled(boolean value) { handled = value; }
    public boolean isCancelled() { return cancelled; }
    public void setCancelled(boolean value) { cancelled = value; }
    public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
