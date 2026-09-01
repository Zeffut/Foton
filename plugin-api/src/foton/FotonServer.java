package foton;

import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Locale;
import java.util.UUID;
import java.util.logging.Logger;
import org.bukkit.Server;
import org.bukkit.World;
import org.bukkit.command.CommandSender;
import org.bukkit.entity.Player;
import org.bukkit.event.Listener;
import org.bukkit.plugin.Plugin;
import org.bukkit.plugin.PluginManager;
import org.bukkit.plugin.messaging.Messenger;
import org.bukkit.scheduler.BukkitScheduler;

/** The one Server, answering out of Foton. */
public final class FotonServer implements Server {
    private final PluginManager plugins = new Plugins();
    private final Messenger channels = new Channels();
    private final BukkitScheduler scheduler = new FotonScheduler();
    private final Logger logger = Logger.getLogger("Foton");

    @Override
    public PluginManager getPluginManager() {
        return plugins;
    }

    @Override
    public Messenger getMessenger() {
        return channels;
    }

    @Override
    public BukkitScheduler getScheduler() {
        return scheduler;
    }

    @Override
    public Collection<? extends Player> getOnlinePlayers() {
        List<Player> online = new ArrayList<>();
        for (String id : Native.onlinePlayerIds()) {
            UUID parsed = Native.parse(id);
            if (parsed != null) {
                online.add(new FotonPlayer(parsed));
            }
        }
        return online;
    }

    /** Bukkit matches a partial name here, and plugins depend on it: `/msg ad`
     * finding Ada is this method, not the command that calls it. */
    @Override
    public Player getPlayer(String name) {
        if (name == null || name.isEmpty()) {
            return null;
        }
        Player exact = getPlayerExact(name);
        if (exact != null) {
            return exact;
        }
        String wanted = name.toLowerCase(Locale.ROOT);
        Player best = null;
        int shortest = Integer.MAX_VALUE;
        for (Player player : getOnlinePlayers()) {
            String candidate = player.getName().toLowerCase(Locale.ROOT);
            if (candidate.startsWith(wanted) && candidate.length() < shortest) {
                best = player;
                shortest = candidate.length();
            }
        }
        return best;
    }

    @Override
    public Player getPlayerExact(String name) {
        String id = Native.playerIdByName(name);
        if (id == null) {
            return null;
        }
        UUID parsed = Native.parse(id);
        return parsed == null ? null : new FotonPlayer(parsed);
    }

    @Override
    public Player getPlayer(UUID id) {
        if (id == null) {
            return null;
        }
        FotonPlayer player = new FotonPlayer(id);
        return player.isOnline() ? player : null;
    }

    @Override
    public String getName() {
        return Native.serverName();
    }

    @Override
    public String getVersion() {
        return Native.serverBrand();
    }

    /** The API version a plugin checks against, not Foton's own.
     *
     * A plugin reading this is asking "which Bukkit am I talking to", and
     * answering with Foton's version number would tell it something true about
     * the wrong question.
     */
    @Override
    public String getBukkitVersion() {
        return Native.serverVersion();
    }

    @Override
    public boolean getOnlineMode() {
        return Native.onlineMode();
    }

    @Override
    public int getMaxPlayers() {
        return Native.maxPlayers();
    }

    @Override
    public Logger getLogger() {
        return logger;
    }

    @Override
    public CommandSender getConsoleSender() {
        return ConsoleSender.INSTANCE;
    }

    @Override
    public List<World> getWorlds() {
        List<World> worlds = new ArrayList<>();
        for (String name : Native.worldNames()) {
            worlds.add(new FotonWorld(name));
        }
        return worlds;
    }

    @Override
    public World getWorld(String name) {
        if (name == null) {
            return null;
        }
        for (String candidate : Native.worldNames()) {
            if (candidate.equals(name)) {
                return new FotonWorld(candidate);
            }
        }
        return null;
    }

    @Override
    public int broadcastMessage(String message) {
        return Native.broadcast(message);
    }

    /** Runs a command as somebody. Only plugin commands: Foton's own
     * dispatcher runs on the tick thread and this can be called from anywhere,
     * so reaching into it from here would be the race the scheduler exists to
     * prevent. */
    @Override
    public boolean dispatchCommand(CommandSender sender, String command) {
        return CommandMap.dispatch(sender, command);
    }

    private static final class Plugins implements PluginManager {
        @Override public void registerEvents(Listener listener, Plugin plugin) {
            EventBridge.register(listener, plugin);
        }

        @Override public Plugin getPlugin(String name) {
            return PluginHost.byName(name);
        }

        @Override public Plugin[] getPlugins() {
            return PluginHost.all();
        }
    }

    private static final class Channels implements Messenger {
        @Override public void registerOutgoingPluginChannel(Plugin source, String channel) {
            System.out.println("[server] " + source.getName() + " opened channel " + channel);
        }

        @Override public void unregisterOutgoingPluginChannel(Plugin source, String channel) {
            System.out.println("[server] " + source.getName() + " closed channel " + channel);
        }
    }
}
