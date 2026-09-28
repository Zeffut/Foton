package org.bukkit.command;

import java.util.List;
import org.bukkit.plugin.Plugin;

/** A command a plugin declared in its plugin.yml. */
public final class PluginCommand extends Command implements PluginIdentifiableCommand {
    private final Plugin owner;
    private CommandExecutor executor;
    private TabCompleter completer;

    public PluginCommand(String name, Plugin owner) {
        super(name);
        this.owner = owner;
        // The plugin is its own default handler, which is how most plugins
        // are written: they override onCommand and never call setExecutor.
        this.executor = owner;
    }

    @Override
    public Plugin getPlugin() {
        return owner;
    }

    /** Null gives the command back to its plugin, as in Bukkit. */
    public void setExecutor(CommandExecutor value) {
        this.executor = value == null ? owner : value;
    }

    public CommandExecutor getExecutor() {
        return executor;
    }

    public void setTabCompleter(TabCompleter value) {
        this.completer = value;
    }

    public TabCompleter getTabCompleter() {
        return completer;
    }

    /** Runs the plugin's handler, and shows the usage line if it declined.
     *
     * Returning false from `onCommand` means "the arguments were wrong", and
     * Bukkit answers it by printing the usage from plugin.yml. Plugins rely on
     * that: a great many of them have no argument checking beyond it.
     */
    @Override
    public boolean execute(CommandSender sender, String label, String[] args) {
        if (!owner.isEnabled()) {
            sender.sendMessage("This command belongs to a plugin that is not enabled.");
            return true;
        }
        if (!testPermission(sender)) {
            return true;
        }
        if (executor == null) {
            return false;
        }

        boolean handled;
        try {
            handled = executor.onCommand(sender, this, label, args);
        } catch (Throwable error) {
            // One plugin's bug is not the server's problem, and an exception
            // crossing back into Foton is a crash rather than a message.
            sender.sendMessage("That command failed. See the console.");
            System.out.println("[command] " + owner.getName() + " threw on /" + label + ": "
                + error);
            error.printStackTrace(System.out);
            return true;
        }

        if (!handled && !getUsage().isEmpty()) {
            for (String line : getUsage().replace("<command>", label).split("\n")) {
                sender.sendMessage(line);
            }
        }
        return true;
    }

    /** Bukkit's order: the completer, then the executor if it completes
     * too, then player names when neither had an answer. */
    @Override
    public List<String> tabComplete(CommandSender sender, String label, String[] args) {
        List<String> answer = null;
        try {
            if (completer != null) {
                answer = completer.onTabComplete(sender, this, label, args);
            }
            if (answer == null && executor instanceof TabCompleter tabs) {
                answer = tabs.onTabComplete(sender, this, label, args);
            }
        } catch (Throwable error) {
            System.out.println("[command] " + owner.getName() + " threw completing /" + label
                + ": " + error);
            return List.of();
        }
        return answer == null ? super.tabComplete(sender, label, args) : answer;
    }
}
