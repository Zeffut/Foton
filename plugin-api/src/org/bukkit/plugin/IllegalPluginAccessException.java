package org.bukkit.plugin;

/** Thrown when a plugin attempts an operation outside its enabled lifetime. */
@SuppressWarnings("serial")
public class IllegalPluginAccessException extends RuntimeException {
    public IllegalPluginAccessException() {}

    public IllegalPluginAccessException(String message) {
        super(message);
    }

    @Override
    public String getMessage() {
        return super.getMessage();
    }
}
