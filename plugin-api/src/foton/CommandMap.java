package foton;

import java.util.LinkedHashMap;
import java.util.Locale;
import java.util.Map;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;
import org.bukkit.command.PluginCommand;
import org.bukkit.plugin.Plugin;
import io.papermc.paper.command.brigadier.Commands;

/** Every command name a plugin has claimed.
 *
 * Foton offers each typed command here before parsing it. A name nobody
 * claimed is answered "not mine" and the server carries on to its own
 * dispatcher, which is what keeps a plugin from shadowing a vanilla command it
 * never asked for.
 */
public final class CommandMap {
    private static final Map<String, Command> byName = new LinkedHashMap<>();
    private static final Map<Plugin, Commands> brigadierByPlugin = new java.util.IdentityHashMap<>();

    private CommandMap() {}

    /** Claims a name, and its aliases, for a plugin's command.
     *
     * The first plugin to claim a name keeps it. Bukkit prefixes the loser's
     * command with its plugin name instead of dropping it; that is worth doing
     * once something can list commands, and until then the loser is announced
     * rather than silently ignored.
     */
    public static void register(Command command) {
        put(command.getName(), command);
        for (String alias : command.getAliases()) {
            put(alias, command);
        }
    }

    private static void put(String name, Command command) {
        String key = key(name);
        if (key.isEmpty()) {
            return;
        }
        Command existing = byName.get(key);
        if (existing != null && existing != command) {
            System.out.println("[command] /" + key + " is already claimed; "
                + owner(command) + " will not get it");
            return;
        }
        byName.put(key, command);
    }

    private static String owner(Command command) {
        if (command instanceof PluginCommand) {
            Plugin plugin = ((PluginCommand) command).getPlugin();
            return plugin == null ? "a plugin" : plugin.getName();
        }
        return "a plugin";
    }

    /** Drops everything a plugin claimed, for a disable. */
    public static void forget(Plugin plugin) {
        byName.entrySet().removeIf(entry -> entry.getValue() instanceof PluginCommand
            && ((PluginCommand) entry.getValue()).getPlugin() == plugin);
        brigadierByPlugin.remove(plugin);
    }

    /** Retains a plugin's lifecycle Brigadier registrar for command dispatch. */
    public static void registerBrigadier(Commands commands, Plugin plugin) {
        brigadierByPlugin.put(plugin, commands);
    }

    public static void clear() {
        byName.clear();
    }

    /** Snapshot exposed through Bukkit's administrative command map API. */
    public static Map<String, Command> knownCommands() {
        return new LinkedHashMap<>(byName);
    }

    public static Command get(String name) {
        return byName.get(key(name));
    }
    public static java.util.List<String> tabComplete(CommandSender sender, String buffer) {
        String line = buffer == null ? "" : buffer;
        String trimmed = line.trim();
        if (trimmed.isEmpty() || !line.endsWith(" ")) {
            String prefix = trimmed.toLowerCase(Locale.ROOT);
            java.util.List<String> result = new java.util.ArrayList<>();
            for (String name : byName.keySet()) if (name.startsWith(prefix)) result.add(name);
            return result;
        }
        String[] parts = trimmed.split("\\s+");
        Command command = get(parts[0]);
        if (command == null) return java.util.Collections.emptyList();
        String[] args = new String[Math.max(0, parts.length - 1)];
        if (args.length > 0) System.arraycopy(parts, 1, args, 0, args.length);
        java.util.List<String> completions;
        try {
            completions = command.tabComplete(sender, parts[0], args);
        } catch (Throwable ignored) {
            completions = java.util.Collections.emptyList();
        }
        org.bukkit.event.server.TabCompleteEvent event =
            new org.bukkit.event.server.TabCompleteEvent(sender, line, completions);
        org.bukkit.Bukkit.getPluginManager().callEvent(event);
        return event.isCancelled() ? java.util.Collections.emptyList() : event.getCompletions();
    }

    public static boolean unregister(Command command) {
        boolean removed = byName.entrySet().removeIf(entry -> entry.getValue() == command);
        return removed;
    }

    private static String key(String name) {
        return name == null ? "" : name.trim().toLowerCase(Locale.ROOT);
    }

    /** Runs a whole command line, and says whether anyone owned it.
     *
     * The line arrives without its slash, exactly as typed. Splitting on runs
     * of spaces rather than single ones matches Bukkit, and matters because a
     * player who double-taps the space bar should not send an empty argument.
     */
    public static boolean dispatch(CommandSender sender, String line) {
        String trimmed = line == null ? "" : line.trim();
        if (trimmed.isEmpty()) {
            return false;
        }
        String[] parts = trimmed.split("\\s+");
        Command command = get(parts[0]);
        if (command == null) {
            for (Commands brigadier : brigadierByPlugin.values()) {
                if (brigadier.dispatch(sender, trimmed)) return true;
            }
            return false;
        }
        String[] args = new String[parts.length - 1];
        System.arraycopy(parts, 1, args, 0, args.length);
        try {
            command.execute(sender, parts[0], args);
        } catch (Throwable error) {
            sender.sendMessage("That command failed. See the console.");
            System.out.println("[command] /" + parts[0] + " threw: " + error);
        }
        // Owned either way: a handler that failed still means the server must
        // not go on to report the command as unknown.
        return true;
    }
}
