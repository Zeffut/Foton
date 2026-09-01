package foton;

import java.util.Set;
import java.util.UUID;
import org.bukkit.World;
import org.bukkit.entity.Player;
import org.bukkit.plugin.Plugin;

/** A player, as a plugin holds one.
 *
 * Nothing but a UUID and a route back into Foton. Bukkit's own Player objects
 * behave the same way once their player has left -- they keep answering, and
 * what they answer stops meaning anything -- so a handle that stops resolving
 * is not a new hazard for a plugin to learn.
 */
public final class FotonPlayer implements Player {
    private final UUID id;

    public FotonPlayer(UUID id) { this.id = id; }

    @Override public UUID getUniqueId() { return id; }

    @Override public String getName() {
        String name = Native.playerName(id.toString());
        return name == null ? "" : name;
    }

    @Override public World getWorld() {
        String name = Native.playerWorld(id.toString());
        return name == null ? null : new FotonWorld(name);
    }

    @Override public void sendMessage(String message) {
        Native.sendMessage(id.toString(), message);
    }

    @Override public boolean hasPermission(String permission) {
        return Native.hasPermission(id.toString(), permission);
    }

    @Override public Set<String> getListeningPluginChannels() { return Set.of(); }

    @Override public void sendPluginMessage(Plugin source, String channel, byte[] message) {}

    @Override public boolean equals(Object other) {
        return other instanceof FotonPlayer player && id.equals(player.id);
    }

    @Override public int hashCode() { return id.hashCode(); }

    @Override public String toString() { return "FotonPlayer{" + id + "}"; }
}
