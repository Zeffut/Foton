package org.bukkit.plugin.messaging;

public class ChannelNameTooLongException extends RuntimeException {
    private static final long serialVersionUID = 1L;

    public ChannelNameTooLongException(int length) {
        super("Attempted to use a plugin channel that was " + length
            + " characters; the maximum is " + Messenger.MAX_CHANNEL_SIZE + ".");
    }
}
