package org.bukkit.command.defaults;

import org.bukkit.command.Command;

/** Base class for commands supplied by Bukkit itself. */
public abstract class BukkitCommand extends Command {
    protected BukkitCommand(String name) {
        super(name);
    }
}
