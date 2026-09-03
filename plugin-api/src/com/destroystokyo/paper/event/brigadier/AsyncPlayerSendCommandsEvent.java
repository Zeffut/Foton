package com.destroystokyo.paper.event.brigadier;

import java.util.Objects;
import com.mojang.brigadier.tree.RootCommandNode;
import org.bukkit.entity.Player;
import org.bukkit.event.HandlerList;
import org.bukkit.event.player.PlayerEvent;

/** Paper event exposing the Brigadier command tree sent to a player. */
public class AsyncPlayerSendCommandsEvent<S> extends PlayerEvent {
    private static final HandlerList HANDLERS = new HandlerList();
    private final RootCommandNode<S> commandNode;
    private final boolean firedAsync;

    public AsyncPlayerSendCommandsEvent(Player player, RootCommandNode<S> commandNode,
            boolean hasFiredAsync) {
        super(Objects.requireNonNull(player, "player"));
        this.commandNode = Objects.requireNonNull(commandNode, "commandNode");
        this.firedAsync = hasFiredAsync;
    }

    public RootCommandNode<S> getCommandNode() { return commandNode; }
    public boolean hasFiredAsync() { return firedAsync; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
