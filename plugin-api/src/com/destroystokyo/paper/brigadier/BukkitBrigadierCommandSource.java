package com.destroystokyo.paper.brigadier;

import org.bukkit.command.CommandSender;
import org.bukkit.entity.Entity;

/** Legacy Paper bridge exposing the Bukkit sender for Brigadier integrations. */
public final class BukkitBrigadierCommandSource {
    private final CommandSender sender;
    public BukkitBrigadierCommandSource(CommandSender sender) { this.sender = sender; }
    public CommandSender getBukkitSender() { return sender; }
    public Entity getBukkitEntity() { return sender instanceof Entity entity ? entity : null; }
}
