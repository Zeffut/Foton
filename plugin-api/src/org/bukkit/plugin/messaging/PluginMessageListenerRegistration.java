package org.bukkit.plugin.messaging;

import java.util.Objects;
import org.bukkit.plugin.Plugin;

/** One plugin listener's registration on one custom payload channel. */
public final class PluginMessageListenerRegistration {
    private final Messenger messenger;
    private final Plugin plugin;
    private final String channel;
    private final PluginMessageListener listener;

    public PluginMessageListenerRegistration(
            Messenger messenger, Plugin plugin, String channel, PluginMessageListener listener) {
        this.messenger = Objects.requireNonNull(messenger, "messenger");
        this.plugin = Objects.requireNonNull(plugin, "plugin");
        this.channel = Objects.requireNonNull(channel, "channel");
        this.listener = Objects.requireNonNull(listener, "listener");
    }

    public String getChannel() {
        return channel;
    }

    public PluginMessageListener getListener() {
        return listener;
    }

    public Plugin getPlugin() {
        return plugin;
    }

    public boolean isValid() {
        return messenger.isRegistrationValid(this);
    }

    @Override public boolean equals(Object other) {
        if (!(other instanceof PluginMessageListenerRegistration registration)) {
            return false;
        }
        return messenger.equals(registration.messenger)
            && plugin.equals(registration.plugin)
            && channel.equals(registration.channel)
            && listener.equals(registration.listener);
    }

    @Override public int hashCode() {
        return Objects.hash(messenger, plugin, channel, listener);
    }
}
