package com.destroystokyo.paper.event.brigadier;

import com.mojang.brigadier.tree.LiteralCommandNode;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Legacy Paper event describing a Brigadier command registration. */
public class CommandRegisteredEvent extends Event {
    private final String commandLabel;
    private LiteralCommandNode<?> literal;
    private static final HandlerList HANDLERS = new HandlerList();
    public CommandRegisteredEvent(String commandLabel, LiteralCommandNode<?> literal) {
        this.commandLabel = commandLabel; this.literal = literal;
    }
    public String getCommandLabel() { return commandLabel; }
    public org.bukkit.command.Command getCommand() { return foton.CommandMap.get(commandLabel); }
    public LiteralCommandNode<?> getLiteral() { return literal; }
    /** Returns the registered node through Paper's Brigadier wrapper. */
    public com.destroystokyo.paper.brigadier.BukkitBrigadierCommand getBrigadierCommand() {
        return new com.destroystokyo.paper.brigadier.BukkitBrigadierCommand(literal);
    }
    public void setLiteral(LiteralCommandNode<?> literal) { this.literal = literal; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
