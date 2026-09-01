package org.bukkit.entity;

public interface Player extends org.bukkit.command.CommandSender {
    String getName();
    java.util.UUID getUniqueId();
    org.bukkit.World getWorld();
    java.util.Set<String> getListeningPluginChannels();
    void sendPluginMessage(org.bukkit.plugin.Plugin source, String channel, byte[] message);
}
