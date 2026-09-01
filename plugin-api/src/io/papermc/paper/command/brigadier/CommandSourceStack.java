package io.papermc.paper.command.brigadier;

import org.bukkit.command.CommandSender;

/** Paper command context exposed to lifecycle and Brigadier integrations. */
public final class CommandSourceStack {
    private final CommandSender sender;

    public CommandSourceStack(CommandSender sender) {
        this.sender = sender;
    }

    public CommandSender getSender() {
        return sender;
    }
}
