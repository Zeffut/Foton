package org.bukkit.plugin.messaging;

public class ReservedChannelException extends RuntimeException {
    private static final long serialVersionUID = 1L;

    public ReservedChannelException(String channel) {
        super("Attempted to register for a reserved channel name ('" + channel + "')");
    }
}
