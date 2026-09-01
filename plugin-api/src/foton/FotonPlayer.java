package foton;

import java.util.Set;
import java.util.UUID;
import org.bukkit.Location;
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

    public FotonPlayer(UUID id) {
        this.id = id;
    }

    @Override
    public UUID getUniqueId() {
        return id;
    }

    @Override
    public String getName() {
        String name = Native.playerName(id.toString());
        return name == null ? "" : name;
    }

    @Override
    public World getWorld() {
        String name = Native.playerWorld(id.toString());
        return name == null ? null : new FotonWorld(name);
    }

    /** Where the player is, as of the moment this was asked.
     *
     * The five numbers arrive together rather than one call each, so a plugin
     * cannot read x from one tick and z from the next and end up with a point
     * the player was never at.
     */
    @Override
    public Location getLocation() {
        double[] at = Native.playerPosition(id.toString());
        if (at == null) {
            return null;
        }
        return new Location(getWorld(), at[0], at[1], at[2], (float) at[3], (float) at[4]);
    }

    @Override
    public int getEntityId() {
        // Foton does not hand out network entity ids to plugins: they are a
        // protocol detail that changes on respawn, and a plugin using one as
        // an identity would be wrong in a way that is hard to see.
        return -1;
    }

    @Override
    public boolean isDead() {
        return false;
    }

    @Override
    public boolean isOnline() {
        return Native.playerName(id.toString()) != null;
    }

    @Override
    public void sendMessage(String message) {
        Native.sendMessage(id.toString(), message);
    }

    @Override
    public boolean hasPermission(String permission) {
        return Native.hasPermission(id.toString(), permission);
    }

    @Override
    public Set<String> getListeningPluginChannels() {
        return Set.of();
    }

    @Override
    public void sendPluginMessage(Plugin source, String channel, byte[] message) {}

    @Override
    public boolean equals(Object other) {
        return other instanceof FotonPlayer player && id.equals(player.id);
    }

    @Override
    public int hashCode() {
        return id.hashCode();
    }

    @Override
    public String toString() {
        return "FotonPlayer{" + id + "}";
    }
}
