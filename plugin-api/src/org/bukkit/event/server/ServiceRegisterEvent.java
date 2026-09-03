package org.bukkit.event.server;

import org.bukkit.event.HandlerList;
import org.bukkit.plugin.RegisteredServiceProvider;

/** A service provider was registered. */
public class ServiceRegisterEvent extends ServiceEvent {
    private static final HandlerList HANDLERS = new HandlerList();

    public ServiceRegisterEvent(RegisteredServiceProvider<?> provider) {
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
