package org.bukkit.command;

/**
 * Convenience interface for a command executor that also supplies tab completions.
 */
public interface TabExecutor extends TabCompleter, CommandExecutor {
}
