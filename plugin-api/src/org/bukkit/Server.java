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

    World getWorld(String name);

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
