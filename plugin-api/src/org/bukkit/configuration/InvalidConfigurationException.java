package org.bukkit.configuration;

/** A configuration file that is not valid for its format. */
public class InvalidConfigurationException extends Exception {
    private static final long serialVersionUID = 1L;

    public InvalidConfigurationException() {
        super();
    }

    public InvalidConfigurationException(String message) {
        super(message);
    }

    public InvalidConfigurationException(Throwable cause) {
        super(cause);
    }

    public InvalidConfigurationException(String message, Throwable cause) {
        super(message, cause);
    }

    @Override public void printStackTrace() { super.printStackTrace(); }
}
