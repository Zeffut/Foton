package io.papermc.paper.configuration;

/**
 * Server settings exposed by Paper's server configuration API.
 *
 * <p>Steel currently has no proxy-forwarding mode. Consequently its
 * configuration always reports {@code false}; this describes the active
 * transport configuration, not an unknown value.</p>
 */
public final class ServerConfiguration {
    private final boolean proxyOnlineMode;

    /** Creates a configuration snapshot. */
    public ServerConfiguration(boolean proxyOnlineMode) {
        this.proxyOnlineMode = proxyOnlineMode;
    }

    /** Returns whether authentication is delegated to a proxy. */
    public boolean isProxyOnlineMode() {
        return proxyOnlineMode;
    }
}
