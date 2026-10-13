import foton.CommandMap;
import java.util.ArrayList;
import java.util.List;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;

/** Commands Foton answers itself, as Bukkit and Paper do: {@code /plugins} and its alias {@code /pl}. */
public final class HostCommandsCheck {
    private HostCommandsCheck() {}

    static void check() {
        Command plugins = CommandMap.get("plugins");
        Checks.expect(plugins != null, "/plugins must exist");
        Checks.same(CommandMap.get("pl"), plugins, "/pl is the alias of /plugins");
        Checks.same(CommandMap.get("PL"), plugins, "command names are not case sensitive");

        // A plugin or a reload clearing the map must not take the host's own commands with it.
        CommandMap.clear();
        Checks.same(CommandMap.get("pl"), plugins, "/pl survives a clear of the command map");
        Checks.expect(CommandMap.tabComplete(null, "pl").contains("plugins"), "/pl completes to /plugins");

        List<String> said = new ArrayList<>();
        CommandSender sender = (CommandSender) java.lang.reflect.Proxy.newProxyInstance(
            HostCommandsCheck.class.getClassLoader(), new Class<?>[] {CommandSender.class},
            (proxy, method, arguments) -> {
                if (method.getName().equals("sendMessage") && arguments[0] instanceof String text) said.add(text);
                return method.getReturnType() == boolean.class ? Boolean.TRUE : null;
            });
        Checks.expect(CommandMap.dispatch(sender, "pl"), "/pl is owned, so the server does not call it unknown");
        // What it says is what the host has loaded, sorted by name, whichever plugins other checks left.
        org.bukkit.plugin.Plugin[] loaded = foton.PluginHost.all();
        List<String> expected = new ArrayList<>();
        expected.add("Server Plugins (" + loaded.length + "):");
        if (loaded.length > 0) {
            expected.add("Bukkit Plugins:");
            java.util.Arrays.stream(loaded).map(org.bukkit.plugin.Plugin::getName)
                .sorted(String.CASE_INSENSITIVE_ORDER).forEach(name -> expected.add(" - " + name));
        }
        Checks.same(said, expected, "/pl lists exactly the plugins the host has loaded");

        // A plugin that claims the same name keeps it: it asked for it.
        Command claimed = new Command("plugins") {
            @Override public boolean execute(CommandSender s, String label, String[] args) { return true; }
        };
        CommandMap.register(claimed);
        Checks.same(CommandMap.get("plugins"), claimed, "a plugin's own /plugins wins");
        Checks.same(CommandMap.get("pl"), plugins, "and the alias it did not claim stays Foton's");
        CommandMap.unregister(claimed);
    }
}
