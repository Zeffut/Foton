package org.bukkit.command;

/** Bukkit command map backed by Foton's runtime registry. */
public final class SimpleCommandMap implements CommandMap {
    public SimpleCommandMap() {}
    public SimpleCommandMap(org.bukkit.Server server) {}
    @Override public boolean register(String fallbackPrefix, Command command) {
        if (command == null) return false;
        foton.CommandMap.register(command);
        return true;
    }
    public Command getCommand(String name) { return foton.CommandMap.get(name); }
    public java.util.List<String> tabComplete(CommandSender sender, String buffer) { return foton.CommandMap.tabComplete(sender, buffer); }
    public void clearCommands() { foton.CommandMap.clear(); }
    public boolean unregister(Command command) {
        return command != null && foton.CommandMap.unregister(command);
    }
}
