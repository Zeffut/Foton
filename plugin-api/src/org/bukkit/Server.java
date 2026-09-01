package org.bukkit;

import java.util.Collection;
import java.util.List;
import java.util.UUID;
import java.util.logging.Logger;
import org.bukkit.command.CommandSender;
import org.bukkit.command.ConsoleCommandSender;
import org.bukkit.entity.Player;
import org.bukkit.plugin.PluginManager;
import org.bukkit.plugin.ServicesManager;
import org.bukkit.plugin.messaging.Messenger;
import org.bukkit.scheduler.BukkitScheduler;

/** What a plugin asks the server for. */
public interface Server {
    PluginManager getPluginManager();

    Messenger getMessenger();

    BukkitScheduler getScheduler();

    ServicesManager getServicesManager();

    boolean isPrimaryThread();

    /** Steel has one global tick thread; it is the Bukkit primary thread. */
    default boolean isGlobalTickThread() {
        return isPrimaryThread();
    }

    /** Saves all connected players asynchronously. */
    default void savePlayers() { }

    /** Requests a graceful server shutdown. */
    default void shutdown() {
        foton.Native.shutdown();
    }

    /**
     * Region ownership collapses to the single serialized tick in Steel.
     * A location without a world cannot belong to any region.
     */
    default boolean isOwnedByCurrentRegion(Location location) {
        return location != null && location.getWorld() != null && isPrimaryThread();
    }

    /** Region ownership for a live entity on Steel's serialized tick. */
    default boolean isOwnedByCurrentRegion(org.bukkit.entity.Entity entity) {
        return entity != null && entity.getWorld() != null && isPrimaryThread();
    }

    Collection<? extends Player> getOnlinePlayers();

    Player getPlayer(String name);

    Player getPlayer(UUID id);

    Player getPlayerExact(String name);

    String getName();

    String getVersion();

    String getBukkitVersion();

    boolean getOnlineMode();

    int getMaxPlayers();

    Logger getLogger();

    ConsoleCommandSender getConsoleSender();

    List<World> getWorlds();

    org.bukkit.scoreboard.ScoreboardManager getScoreboardManager();

    World getWorld(String name);

    default org.bukkit.entity.Entity getEntity(java.util.UUID uuid) {
        if (uuid == null) return null;
        for (World world : getWorlds()) {
            for (org.bukkit.entity.Entity entity : world.getEntities()) {
                if (uuid.equals(entity.getUniqueId())) return entity;
            }
        }
        return null;
    }

    int broadcastMessage(String message);

    void sendPluginMessage(org.bukkit.plugin.Plugin source, String channel, byte[] message);

    boolean dispatchCommand(CommandSender sender, String command);

    OfflinePlayer getOfflinePlayer(String name);

    OfflinePlayer getOfflinePlayer(UUID id);

    org.bukkit.boss.BossBar createBossBar(
        String title,
        org.bukkit.boss.BarColor color,
        org.bukkit.boss.BarStyle style,
        org.bukkit.boss.BarFlag... flags);

    io.papermc.paper.threadedregions.scheduler.GlobalRegionScheduler getGlobalRegionScheduler();

    io.papermc.paper.threadedregions.scheduler.RegionScheduler getRegionScheduler();

    io.papermc.paper.threadedregions.scheduler.AsyncScheduler getAsyncScheduler();
}
