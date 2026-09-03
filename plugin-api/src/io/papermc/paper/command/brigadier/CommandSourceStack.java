package io.papermc.paper.command.brigadier;

import org.bukkit.Location;
import org.bukkit.command.CommandSender;
import org.bukkit.entity.Entity;

/** Source context passed to Brigadier command executions. */
public final class CommandSourceStack {
    private final CommandSender sender;
    private final Location location;
    public CommandSourceStack(CommandSender sender, Location location) {
        this.sender = sender;
        this.location = location;
    }
    public CommandSender getSender() { return sender; }
    public Entity getExecutor() { return sender instanceof Entity entity ? entity : null; }
    public Location getLocation() { return location; }
}
