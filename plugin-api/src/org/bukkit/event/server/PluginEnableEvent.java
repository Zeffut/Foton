package org.bukkit.event.server;

import org.bukkit.plugin.Plugin;

/** A plugin was enabled. */
public class PluginEnableEvent extends PluginEvent {
    public PluginEnableEvent(Plugin plugin) {
        super(plugin);
    }
}
