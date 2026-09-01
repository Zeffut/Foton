package org.bukkit.entity;

import java.util.Set;
import org.bukkit.plugin.Plugin;

/** A player on the server, as a plugin sees one. */
public interface Player extends Entity {
    @Override
    String getName();

    Set<String> getListeningPluginChannels();

    void sendPluginMessage(Plugin source, String channel, byte[] message);

    boolean isOnline();
}
