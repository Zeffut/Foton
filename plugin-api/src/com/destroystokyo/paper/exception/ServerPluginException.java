package com.destroystokyo.paper.exception;

import org.bukkit.plugin.Plugin;

/** Plugin failure carrying the plugin responsible for it. */
public class ServerPluginException extends ServerException {
    private final Plugin responsiblePlugin;
    public ServerPluginException(String message, Plugin plugin) { super(message); this.responsiblePlugin = plugin; }
    public ServerPluginException(String message, Throwable cause, Plugin plugin) { super(message, cause); this.responsiblePlugin = plugin; }
    public ServerPluginException(Throwable cause, Plugin plugin) { super(cause); this.responsiblePlugin = plugin; }
    @Override public synchronized Throwable getCause() { return super.getCause(); }
    public Plugin getResponsiblePlugin() { return responsiblePlugin; }
}
