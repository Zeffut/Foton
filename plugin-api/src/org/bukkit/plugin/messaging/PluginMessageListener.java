package org.bukkit.plugin.messaging;

import org.bukkit.entity.Player;

/** Something a plugin registered to hear a channel. */
public interface PluginMessageListener {
    void onPluginMessageReceived(String channel, Player player, byte[] message);
}
