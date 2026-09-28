package org.bukkit.command;

/** Both halves of a command: running it and completing it. */
public interface TabExecutor extends TabCompleter, CommandExecutor {
}
