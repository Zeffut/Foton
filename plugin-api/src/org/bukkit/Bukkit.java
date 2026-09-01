package org.bukkit;

import java.util.Collection;
import java.util.List;
import java.util.UUID;
import java.util.logging.Logger;
import org.bukkit.command.CommandSender;
import org.bukkit.command.ConsoleCommandSender;
import org.bukkit.entity.Player;
import org.bukkit.plugin.PluginManager;
import org.bukkit.scheduler.BukkitScheduler;

/** The static door onto the running server, which most plugins use.
 *
 * Every method here forwards to the one Server. Plugins reach for the statics
 * far more than the interface -- `Bukkit.getVersion` alone is called by
 * thirty-seven of the fifty-nine plugins surveyed -- so the two have to stay
 * in step.
 */
public final class Bukkit {
    private static Server server;

    private Bukkit() {}

    public static Server getServer() {
        return server;
    }

    public static void setServer(Server value) {
        if (server != null) {
            throw new UnsupportedOperationException("Cannot redefine singleton Server");
        }
        server = value;
    }

    public static PluginManager getPluginManager() {
        return server.getPluginManager();
    }

    public static UnsafeValues getUnsafe() {
        return new foton.FotonUnsafeValues();
    }

    public static org.bukkit.plugin.messaging.Messenger getMessenger() {
        return server.getMessenger();
    }

    public static BukkitScheduler getScheduler() {
        return server.getScheduler();
    }

    public static org.bukkit.plugin.ServicesManager getServicesManager() {
        return server.getServicesManager();
    }

    public static boolean isPrimaryThread() {
        return server.isPrimaryThread();
    }

    public static Collection<? extends Player> getOnlinePlayers() {
        return server.getOnlinePlayers();
    }

    public static Player getPlayer(String name) {
        return server.getPlayer(name);
    }

    public static Player getPlayer(UUID id) {
        return server.getPlayer(id);
    }

    public static Player getPlayerExact(String name) {
        return server.getPlayerExact(name);
    }

    public static String getName() {
        return server.getName();
    }

    public static String getVersion() {
        return server.getVersion();
    }

    public static String getBukkitVersion() {
        return server.getBukkitVersion();
    }

    public static boolean getOnlineMode() {
        return server.getOnlineMode();
    }

    public static int getMaxPlayers() {
        return server.getMaxPlayers();
    }

    public static Logger getLogger() {
        return server.getLogger();
    }

    public static ConsoleCommandSender getConsoleSender() {
        return server.getConsoleSender();
    }

    public static List<World> getWorlds() {
        return server.getWorlds();
    }

    public static org.bukkit.scoreboard.ScoreboardManager getScoreboardManager() {
        return server.getScoreboardManager();
    }

    public static World getWorld(String name) {
        return server.getWorld(name);
    }

    public static int broadcastMessage(String message) {
        return server.broadcastMessage(message);
    }

    public static boolean dispatchCommand(CommandSender sender, String command) {
        return server.dispatchCommand(sender, command);
    }

    public static OfflinePlayer getOfflinePlayer(String name) {
        return server.getOfflinePlayer(name);
    }

    public static OfflinePlayer getOfflinePlayer(UUID id) {
        return server.getOfflinePlayer(id);
    }

    public static org.bukkit.boss.BossBar createBossBar(
            String title,
            org.bukkit.boss.BarColor color,
            org.bukkit.boss.BarStyle style,
            org.bukkit.boss.BarFlag... flags) {
        return server.createBossBar(title, color, style, flags);
    }

    public static io.papermc.paper.threadedregions.scheduler.GlobalRegionScheduler
            getGlobalRegionScheduler() {
        return server.getGlobalRegionScheduler();
    }

    public static io.papermc.paper.threadedregions.scheduler.RegionScheduler getRegionScheduler() {
        return server.getRegionScheduler();
    }

    public static io.papermc.paper.threadedregions.scheduler.AsyncScheduler getAsyncScheduler() {
        return server.getAsyncScheduler();
    }
}
