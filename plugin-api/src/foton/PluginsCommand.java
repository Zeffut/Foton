package foton;

import java.util.Arrays;
import java.util.Comparator;
import java.util.List;
import net.kyori.adventure.audience.Audience;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.format.NamedTextColor;
import net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;
import org.bukkit.plugin.Plugin;

/** {@code /plugins}, and its alias {@code /pl}: the plugins the server has loaded.
 *
 * <p>Bukkit and Paper both answer this one themselves, so a server without it looks broken to anyone
 * who has run one. The layout is Paper's: a count, then the names, green for a plugin that is enabled
 * and red for one that failed to enable or was disabled. Every plugin Foton loads is a Bukkit-style
 * plugin, so there is a single group.
 */
final class PluginsCommand extends Command {
    PluginsCommand() {
        super("plugins", "Gets a list of plugins running on the server", "/plugins", List.of("pl"));
        setPermission("bukkit.command.plugins");
    }

    @Override public boolean execute(CommandSender sender, String label, String[] args) {
        Plugin[] plugins = PluginHost.all();
        Arrays.sort(plugins, Comparator.comparing(Plugin::getName, String.CASE_INSENSITIVE_ORDER));
        send(sender, Component.text("Server Plugins (" + plugins.length + "):", NamedTextColor.WHITE));
        if (plugins.length == 0) return true;
        send(sender, Component.text("Bukkit Plugins:", NamedTextColor.GOLD));
        for (Plugin plugin : plugins) {
            send(sender, Component.text(" - ", NamedTextColor.DARK_GRAY).append(Component.text(
                plugin.getName(), plugin.isEnabled() ? NamedTextColor.GREEN : NamedTextColor.RED)));
        }
        return true;
    }

    /** Sends the line with its colors to a player, and as plain text to anything that cannot show them. */
    private static void send(CommandSender sender, Component line) {
        if (sender instanceof Audience audience) audience.sendMessage(line);
        else sender.sendMessage(PlainTextComponentSerializer.plainText().serialize(line));
    }
}
