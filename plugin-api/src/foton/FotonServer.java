package foton;

import java.util.Collection;
import java.util.List;
import org.bukkit.Server;
import org.bukkit.entity.Player;
import org.bukkit.event.Listener;
import org.bukkit.plugin.Plugin;
import org.bukkit.plugin.PluginManager;
import org.bukkit.plugin.messaging.Messenger;
import org.bukkit.scheduler.BukkitScheduler;
import org.bukkit.scheduler.BukkitTask;

/** The server a plugin sees.
 *
 * Everything here still answers from Java. The point of this step is that a
 * real plugin loads and enables at all; the calls become native ones into
 * Foton next, and this class is the seam where that happens.
 */
public final class FotonServer implements Server {
    private final PluginManager plugins = new Plugins();
    private final Messenger messenger = new Channels();
    private final BukkitScheduler scheduler = new Scheduler();

    @Override public PluginManager getPluginManager() { return plugins; }
    @Override public Messenger getMessenger() { return messenger; }
    @Override public BukkitScheduler getScheduler() { return scheduler; }
    @Override public Collection<? extends Player> getOnlinePlayers() {
        String[] ids = Native.onlinePlayerIds();
        List<Player> players = new java.util.ArrayList<>(ids.length);
        for (String id : ids) {
            java.util.UUID parsed = Native.parse(id);
            if (parsed != null) {
                players.add(new FotonPlayer(parsed));
            }
        }
        return java.util.Collections.unmodifiableList(players);
    }

    @Override public String getName() { return Native.serverName(); }
    @Override public String getVersion() { return Native.serverVersion(); }

    private static final class Plugins implements PluginManager {
        @Override public void registerEvents(Listener listener, Plugin plugin) {
            System.out.println("[server] " + plugin.getName() + " registered "
                + listener.getClass().getName());
        }
        @Override public Plugin getPlugin(String name) { return null; }
        @Override public Plugin[] getPlugins() { return new Plugin[0]; }
    }

    private static final class Channels implements Messenger {
        @Override public void registerOutgoingPluginChannel(Plugin source, String channel) {
            System.out.println("[server] " + source.getName() + " opened channel " + channel);
        }
        @Override public void unregisterOutgoingPluginChannel(Plugin source, String channel) {
            System.out.println("[server] " + source.getName() + " closed channel " + channel);
        }
    }

    private static final class Scheduler implements BukkitScheduler {
        @Override public BukkitTask runTask(Plugin plugin, Runnable task) {
            task.run();
            return new Task();
        }
        @Override public BukkitTask runTaskLater(Plugin plugin, Runnable task, long delayTicks) {
            return new Task();
        }
    }

    private static final class Task implements BukkitTask {
        @Override public int getTaskId() { return 0; }
        @Override public void cancel() {}
    }
}
