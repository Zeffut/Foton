package org.bukkit.event.server;

import org.bukkit.plugin.Plugin;

/** A plugin was disabled.
 *
 * Plugins that hook each other listen for this so they can let go of a handle
 * before it stops meaning anything.
 */
public class PluginDisableEvent extends PluginEvent {
    public PluginDisableEvent(Plugin plugin) {
        super(plugin);
    }
}
