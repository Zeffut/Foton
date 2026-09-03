package org.bukkit.plugin;

/** A plugin.yml that cannot be read as one. */
public class InvalidDescriptionException extends Exception {
    private static final long serialVersionUID = 1L;

    public InvalidDescriptionException(String message) {
        super(message);
    }

    public InvalidDescriptionException(Throwable cause) {
        super(cause);
    }
}
