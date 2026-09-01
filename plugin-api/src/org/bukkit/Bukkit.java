package org.bukkit;

/** The static door onto the running server, which most plugins use. */
public final class Bukkit {
    private static Server server;

    private Bukkit() {}

    public static Server getServer() { return server; }

    public static void setServer(Server value) {
        if (server != null) {
            throw new UnsupportedOperationException("Cannot redefine singleton Server");
        }
        server = value;
    }

    public static org.bukkit.plugin.PluginManager getPluginManager() {
        return server.getPluginManager();
    }

    public static org.bukkit.scheduler.BukkitScheduler getScheduler() {
        return server.getScheduler();
    }

    public static java.util.Collection<? extends org.bukkit.entity.Player> getOnlinePlayers() {
        return server.getOnlinePlayers();
    }
}
