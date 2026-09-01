package org.bukkit;

import org.bukkit.plugin.PluginManager;
import org.bukkit.plugin.messaging.Messenger;

/** What a plugin asks the server for. */
public interface Server {
    PluginManager getPluginManager();
    Messenger getMessenger();
    java.util.Collection<? extends org.bukkit.entity.Player> getOnlinePlayers();
    org.bukkit.scheduler.BukkitScheduler getScheduler();
    String getName();
    String getVersion();
}
