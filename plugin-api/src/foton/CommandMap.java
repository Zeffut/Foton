package foton;

import java.util.LinkedHashMap;
import java.util.Locale;
import java.util.Map;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;
import org.bukkit.command.PluginCommand;
import org.bukkit.plugin.Plugin;

/** Every command name a plugin has claimed.
 *
 * Foton offers each typed command here before parsing it. A name nobody
 * claimed is answered "not mine" and the server carries on to its own
 * dispatcher, which is what keeps a plugin from shadowing a vanilla command it
 * never asked for.
 */
public final class CommandMap {
    private static final Map<String, Command> byName = new LinkedHashMap<>();

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
    }

    public static void clear() {
        byName.clear();
    }

    public static Command get(String name) {
        return byName.get(key(name));
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
