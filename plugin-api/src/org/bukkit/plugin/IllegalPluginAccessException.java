package org.bukkit.plugin;

/** Thrown when a plugin asks the server for something while it is not
 * enabled -- scheduling a task or registering a listener from `onDisable`,
 * most often. Plugins catch it to fall back to doing the work themselves. */
public class IllegalPluginAccessException extends RuntimeException {
    private static final long serialVersionUID = 1L;

    public IllegalPluginAccessException() {}

    public IllegalPluginAccessException(String msg) {
        super(msg);
    }
}
