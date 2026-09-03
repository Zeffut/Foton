package org.bukkit.plugin.messaging;

import java.util.Set;
import org.bukkit.entity.Player;
import org.bukkit.plugin.Plugin;

/** Custom payloads to and from clients and proxies. */
public interface Messenger {
    int MAX_MESSAGE_SIZE = 1_048_576;
    int MAX_CHANNEL_SIZE = Integer.getInteger("paper.maxCustomChannelName", 32_767);

    boolean isReservedChannel(String channel);

    void registerOutgoingPluginChannel(Plugin source, String channel);

    void unregisterOutgoingPluginChannel(Plugin source, String channel);

    void unregisterOutgoingPluginChannel(Plugin source);

    PluginMessageListenerRegistration registerIncomingPluginChannel(
        Plugin source, String channel, PluginMessageListener listener);

    void unregisterIncomingPluginChannel(
        Plugin source, String channel, PluginMessageListener listener);

    void unregisterIncomingPluginChannel(Plugin source, String channel);

    void unregisterIncomingPluginChannel(Plugin source);

    Set<String> getOutgoingChannels();

    Set<String> getOutgoingChannels(Plugin source);

    Set<String> getIncomingChannels();

    Set<String> getIncomingChannels(Plugin source);

    Set<PluginMessageListenerRegistration> getIncomingChannelRegistrations(Plugin source);

    Set<PluginMessageListenerRegistration> getIncomingChannelRegistrations(String channel);

    Set<PluginMessageListenerRegistration> getIncomingChannelRegistrations(
        Plugin source, String channel);

    boolean isRegistrationValid(PluginMessageListenerRegistration registration);

    boolean isOutgoingChannelRegistered(Plugin source, String channel);

    boolean isIncomingChannelRegistered(Plugin source, String channel);

    void dispatchIncomingMessage(Player source, String channel, byte[] message);
}
