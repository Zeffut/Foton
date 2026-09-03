package org.bukkit.plugin;

import org.bukkit.event.Listener;

public interface PluginManager {
    void registerEvents(Listener listener, Plugin plugin);
    Plugin getPlugin(String name);
    Plugin[] getPlugins();
    default org.bukkit.permissions.Permission getPermission(String name) { return null; }
    default void addPermission(org.bukkit.permissions.Permission permission) { }
    default void removePermission(String name) { }
    default java.util.Set<org.bukkit.permissions.Permissible> getPermissionSubscriptions(String permission) { return java.util.Collections.emptySet(); }

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
        EventExecutor executor,
        Plugin plugin);

    void registerEvent(
        Class<? extends org.bukkit.event.Event> event,
        org.bukkit.event.Listener listener,
        org.bukkit.event.EventPriority priority,
        EventExecutor executor,
        Plugin plugin,
        boolean ignoreCancelled);
}
