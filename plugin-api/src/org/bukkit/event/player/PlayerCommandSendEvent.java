package org.bukkit.event.player;

import java.util.Collection;
import java.util.Objects;
import org.bukkit.entity.Player;
import org.bukkit.event.HandlerList;

/** Fired before the top-level command list is sent to a player. */
public class PlayerCommandSendEvent extends PlayerEvent {
    private static final HandlerList HANDLERS = new HandlerList();
    private final Collection<String> commands;

    public PlayerCommandSendEvent(Player player, Collection<String> commands) {
        super(Objects.requireNonNull(player, "player"));
        this.commands = Objects.requireNonNull(commands, "commands");
    }

    public Collection<String> getCommands() { return commands; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
