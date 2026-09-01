package org.bukkit.command;

/** A command a plugin declared in its plugin.yml. */
public final class PluginCommand extends Command {
    private final org.bukkit.plugin.Plugin owner;
    private CommandExecutor executor;
    private TabCompleter completer;

    public PluginCommand(String name, org.bukkit.plugin.Plugin owner) {
        super(name);
        this.owner = owner;
        this.executor = owner instanceof CommandExecutor ? (CommandExecutor) owner : null;
    }

    public org.bukkit.plugin.Plugin getPlugin() { return owner; }

    public void setExecutor(CommandExecutor value) { this.executor = value; }
    public CommandExecutor getExecutor() { return executor; }

    public void setTabCompleter(TabCompleter value) { this.completer = value; }
    public TabCompleter getTabCompleter() { return completer; }
}
