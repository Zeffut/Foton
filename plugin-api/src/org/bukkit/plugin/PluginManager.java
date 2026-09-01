package org.bukkit.plugin;

import org.bukkit.event.Listener;

public interface PluginManager {
    void registerEvents(Listener listener, Plugin plugin);
    Plugin getPlugin(String name);
    Plugin[] getPlugins();

    /** Fires an event at every handler registered for it. */
    void callEvent(org.bukkit.event.Event event);
}
