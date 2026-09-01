package org.bukkit.plugin.messaging;

public class ChannelNotRegisteredException extends RuntimeException {
    private static final long serialVersionUID = 1L;

    public ChannelNotRegisteredException(String channel) {
        super("Attempted to send a plugin message through the unregistered channel '"
            + channel + "'.");
    }
}
