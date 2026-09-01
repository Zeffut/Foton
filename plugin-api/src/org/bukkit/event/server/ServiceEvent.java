package org.bukkit.event.server;

import org.bukkit.plugin.RegisteredServiceProvider;

/** An event about one service provider registration. */
public abstract class ServiceEvent extends ServerEvent {
    private final RegisteredServiceProvider<?> provider;

    protected ServiceEvent(RegisteredServiceProvider<?> provider) {
        this.provider = provider;
    }

    public RegisteredServiceProvider<?> getProvider() {
        return provider;
    }
}
