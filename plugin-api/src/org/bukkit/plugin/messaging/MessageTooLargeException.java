package org.bukkit.plugin.messaging;

public class MessageTooLargeException extends RuntimeException {
    private static final long serialVersionUID = 1L;

    public MessageTooLargeException(byte[] message) {
        super("Attempted to send a plugin message that was " + message.length
            + " bytes; the maximum is " + Messenger.MAX_MESSAGE_SIZE + " bytes.");
    }
}
