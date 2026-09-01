package org.bukkit.plugin;

import org.bukkit.event.Listener;

public interface PluginManager {
    void registerEvents(Listener listener, Plugin plugin);
    Plugin getPlugin(String name);
    Plugin[] getPlugins();

    /** Fires an event at every handler registered for it. */
    void callEvent(org.bukkit.event.Event event);

    boolean isPluginEnabled(String name);

    boolean isPluginEnabled(Plugin plugin);

    void disablePlugin(Plugin plugin);

    /** Registers one handler by hand, for a plugin building its listeners at
     * runtime rather than annotating them. */
    void registerEvent(
        Class<? extends org.bukkit.event.Event> event,
        org.bukkit.event.Listener listener,
        org.bukkit.event.EventPriority priority,
        org.bukkit.event.EventExecutor executor,
        Plugin plugin);
}
