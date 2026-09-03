package org.bukkit.command;

/** Registry used by plugins to add commands at runtime. */
public interface CommandMap {
    boolean register(String fallbackPrefix, Command command);
    default boolean register(String label, String fallbackPrefix, Command command) {
        return register(fallbackPrefix, command);
    }
    default void registerAll(String fallbackPrefix, java.util.List<Command> commands) {
        if (commands == null) return;
        for (Command command : commands) register(fallbackPrefix, command);
    }
    default Command getCommand(String name) { return foton.CommandMap.get(name); }
    default java.util.Map<String, Command> getKnownCommands() { return foton.CommandMap.knownCommands(); }
    /** Removes all registered plugin commands. */
    default void clearCommands() { foton.CommandMap.clear(); }
    default boolean dispatch(CommandSender sender, String commandLine) { return foton.CommandMap.dispatch(sender, commandLine); }
    /** Completes a command line using the registered commands and command callback. */
    default java.util.List<String> tabComplete(CommandSender sender, String buffer) {
        return foton.CommandMap.tabComplete(sender, buffer);
    }
}
