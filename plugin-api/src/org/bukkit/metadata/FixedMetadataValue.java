package org.bukkit.metadata;

import org.bukkit.plugin.Plugin;

/** Immutable metadata value supplied by a plugin. */
public class FixedMetadataValue implements MetadataValue {
    private final Plugin plugin;
    private final Object value;
    public FixedMetadataValue(Plugin plugin, Object value) { this.plugin = plugin; this.value = value; }
    @Override public Object value() { return value; }
    @Override public Plugin getOwningPlugin() { return plugin; }
}
