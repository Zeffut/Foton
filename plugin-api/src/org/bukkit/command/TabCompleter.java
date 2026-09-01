package org.bukkit.command;

public interface TabCompleter {
    java.util.List<String> onTabComplete(
        CommandSender sender, Command command, String label, String[] args);
}
