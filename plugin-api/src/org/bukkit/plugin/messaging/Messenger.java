package org.bukkit.plugin.messaging;

import org.bukkit.plugin.Plugin;

public interface Messenger {
    void registerOutgoingPluginChannel(Plugin source, String channel);
    void unregisterOutgoingPluginChannel(Plugin source, String channel);
}
