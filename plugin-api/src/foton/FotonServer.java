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
import org.bukkit.command.ConsoleCommandSender;
import org.bukkit.entity.Player;
import org.bukkit.event.Listener;
import org.bukkit.plugin.Plugin;
import org.bukkit.plugin.PluginManager;
import org.bukkit.plugin.ServicesManager;
import org.bukkit.plugin.SimpleServicesManager;
import org.bukkit.plugin.messaging.Messenger;
import org.bukkit.plugin.messaging.PluginMessageListener;
import org.bukkit.scheduler.BukkitScheduler;

/** The one Server, answering out of Foton. */
public final class FotonServer implements Server {
    private final PluginManager plugins = new Plugins();
    private final Messenger channels = new Channels();
    private final BukkitScheduler scheduler = new FotonScheduler();
    private final ServicesManager services = new SimpleServicesManager();
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
    public ServicesManager getServicesManager() {
        return services;
    }

    @Override
    public boolean isPrimaryThread() {
        return Native.isPrimaryThread();
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
    public ConsoleCommandSender getConsoleSender() {
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

    @Override
    public org.bukkit.OfflinePlayer getOfflinePlayer(String name) {
        Player online = getPlayerExact(name);
        return new FotonOfflinePlayer(online == null ? null : online.getUniqueId(), name);
    }

    @Override
    public org.bukkit.OfflinePlayer getOfflinePlayer(UUID id) {
        return new FotonOfflinePlayer(id, null);
    }

    @Override
    public io.papermc.paper.threadedregions.scheduler.GlobalRegionScheduler
            getGlobalRegionScheduler() {
        return FotonRegionSchedulers.GLOBAL;
    }

    @Override
    public io.papermc.paper.threadedregions.scheduler.RegionScheduler getRegionScheduler() {
        return FotonRegionSchedulers.REGION;
    }

    @Override
    public io.papermc.paper.threadedregions.scheduler.AsyncScheduler getAsyncScheduler() {
        return FotonRegionSchedulers.ASYNC;
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

        @Override public void callEvent(org.bukkit.event.Event event) {
            EventBridge.dispatch(event);
        }

        @Override public boolean isPluginEnabled(String name) {
            Plugin plugin = PluginHost.byName(name);
            return plugin != null && plugin.isEnabled();
        }

        @Override public boolean isPluginEnabled(Plugin plugin) {
            return plugin != null && plugin.isEnabled();
        }

        @Override public void disablePlugin(Plugin plugin) {
            PluginHost.disable(plugin);
        }

        @Override public void registerEvent(
                Class<? extends org.bukkit.event.Event> event,
                Listener listener,
                org.bukkit.event.EventPriority priority,
                org.bukkit.plugin.EventExecutor executor,
                Plugin plugin) {
            EventBridge.register(listener, event, priority, executor, plugin);
        }

        @Override public void registerEvent(
                Class<? extends org.bukkit.event.Event> event,
                Listener listener,
                org.bukkit.event.EventPriority priority,
                org.bukkit.plugin.EventExecutor executor,
                Plugin plugin,
                boolean ignoreCancelled) {
            EventBridge.register(
                listener, event, priority, executor, plugin, ignoreCancelled);
        }
    }

    /** Custom payloads.
     *
     * Registration is recorded and nothing is delivered yet: Foton does not
     * hand plugin channel payloads across, so a listener registered here will
     * not be called. The methods exist because a plugin that calls one and
     * finds it missing fails to load at all, and a plugin whose channel is
     * quiet still does everything else it does.
     */
    private static final class Channels implements Messenger {
        private final java.util.Set<String> outgoing = java.util.concurrent.ConcurrentHashMap
            .newKeySet();
        private final java.util.Map<String, PluginMessageListener> incoming =
            new java.util.concurrent.ConcurrentHashMap<>();

        @Override public void registerOutgoingPluginChannel(Plugin source, String channel) {
            outgoing.add(channel);
        }

        @Override public void unregisterOutgoingPluginChannel(Plugin source, String channel) {
            outgoing.remove(channel);
        }

        @Override public void registerIncomingPluginChannel(
                Plugin source, String channel, PluginMessageListener listener) {
            incoming.put(channel, listener);
            System.out.println("[server] " + source.getName() + " is listening on " + channel
                + "; Foton does not deliver plugin messages yet");
        }

        @Override public void unregisterIncomingPluginChannel(Plugin source, String channel) {
            incoming.remove(channel);
        }

        @Override public boolean isOutgoingChannelRegistered(Plugin source, String channel) {
            return outgoing.contains(channel);
        }

        @Override public boolean isIncomingChannelRegistered(Plugin source, String channel) {
            return incoming.containsKey(channel);
        }
    }
}
