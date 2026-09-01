package org.bukkit.plugin.messaging;

import org.bukkit.plugin.Plugin;

/** Custom payloads to and from clients and proxies. */
public interface Messenger {
    void registerOutgoingPluginChannel(Plugin source, String channel);

    void unregisterOutgoingPluginChannel(Plugin source, String channel);

    void registerIncomingPluginChannel(
        Plugin source, String channel, PluginMessageListener listener);

    void unregisterIncomingPluginChannel(Plugin source, String channel);

    boolean isOutgoingChannelRegistered(Plugin source, String channel);

    boolean isIncomingChannelRegistered(Plugin source, String channel);
}
