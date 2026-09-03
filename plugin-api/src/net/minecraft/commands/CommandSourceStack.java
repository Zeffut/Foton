package net.minecraft.commands;

import org.bukkit.command.CommandSender;

/** NMS compatibility source exposed to Paper plugins during command registration. */
public class CommandSourceStack {
    private final CommandSender sender;
    public CommandSourceStack(CommandSender sender) { this.sender = sender; }
    public CommandSender getBukkitSender() { return sender; }
}
