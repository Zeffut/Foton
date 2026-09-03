package org.bukkit.command;

import org.bukkit.plugin.Plugin;

/** A command that can identify the plugin which registered it. */
public interface PluginIdentifiableCommand {
    Plugin getPlugin();
}
