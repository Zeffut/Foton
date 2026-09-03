package org.bukkit.event.server;

import org.bukkit.event.HandlerList;
import org.bukkit.plugin.RegisteredServiceProvider;

/** A service provider was unregistered. */
public class ServiceUnregisterEvent extends ServiceEvent {
    private static final HandlerList HANDLERS = new HandlerList();

    public ServiceUnregisterEvent(RegisteredServiceProvider<?> provider) {
        super(provider);
    }

    @Override
    public HandlerList getHandlers() {
        return HANDLERS;
    }

    public static HandlerList getHandlerList() {
        return HANDLERS;
    }
}
