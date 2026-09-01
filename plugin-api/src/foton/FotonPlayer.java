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
    public org.bukkit.inventory.PlayerInventory getInventory() {
        return new FotonInventory(id.toString());
    }

    @Override
    public org.bukkit.GameMode getGameMode() {
        org.bukkit.GameMode mode = org.bukkit.GameMode.byName(Native.gameMode(id.toString()));
        // A player who has gone is not in any mode; survival is the answer
        // that surprises a plugin least, and Bukkit's own handle to a departed
        // player answers just as arbitrarily.
        return mode == null ? org.bukkit.GameMode.SURVIVAL : mode;
    }

    @Override
    public boolean isOp() {
        return Native.isOperator(id.toString());
    }

    /** The name a plugin may have changed, falling back to the real one.
     *
     * Bukkit stores this per player and Foton has nowhere to put it, so it is
     * kept beside the handle. A plugin that sets it on one handle and reads it
     * from another gets the real name back -- which is wrong, and is the
     * honest consequence of a handle that is only a UUID. Foton needs a place
     * to store it before this can be right.
     */
    @Override
    public String getDisplayName() {
        String chosen = DISPLAY_NAMES.get(id);
        return chosen == null ? getName() : chosen;
    }

    @Override
    public void setDisplayName(String name) {
        if (name == null) {
            DISPLAY_NAMES.remove(id);
        } else {
            DISPLAY_NAMES.put(id, name);
        }
    }

    private static final java.util.Map<UUID, String> DISPLAY_NAMES =
        new java.util.concurrent.ConcurrentHashMap<>();

    @Override
    public void sendTitle(String title, String subtitle, int fadeIn, int stay, int fadeOut) {
        Native.sendTitle(id.toString(), title == null ? "" : title,
            subtitle == null ? "" : subtitle, fadeIn, stay, fadeOut);
    }

    @Override
    public void playSound(org.bukkit.Location at, org.bukkit.Sound sound, float volume,
            float pitch) {
        playSound(at, sound == null ? null : sound.getKey(), volume, pitch);
    }

    @Override
    public void playSound(org.bukkit.Location at, String sound, float volume, float pitch) {
        org.bukkit.Location where = at == null ? getLocation() : at;
        if (where == null || where.getWorld() == null || sound == null) {
            return;
        }
        Native.playSound(where.getWorld().getName(), where.getX(), where.getY(), where.getZ(),
            sound, volume, pitch);
    }

    @Override
    public void playSound(org.bukkit.Location at, org.bukkit.Sound sound,
            org.bukkit.SoundCategory category, float volume, float pitch) {
        playSound(at, sound == null ? null : sound.getKey(), category, volume, pitch);
    }

    @Override
    public void playSound(org.bukkit.Location at, String sound, org.bukkit.SoundCategory category,
            float volume, float pitch) {
        org.bukkit.Location where = at == null ? getLocation() : at;
        if (where == null || where.getWorld() == null || sound == null || category == null) return;
        Native.playSoundCategory(where.getWorld().getName(), where.getX(), where.getY(),
            where.getZ(), sound, category.name(), volume, pitch);
    }

    @Override
    public io.papermc.paper.threadedregions.scheduler.EntityScheduler getScheduler() {
        return FotonRegionSchedulers.forEntity();
    }

    @Override
    public Spigot spigot() {
        return spigot;
    }

    /** Spigot's extra surface, which for Foton is the ordinary one. */
    private final Spigot spigot = new Spigot() {
        @Override
        public void sendMessage(String message) {
            FotonPlayer.this.sendMessage(message);
        }
    };

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
    public void kickPlayer(String message) {
        Native.kickPlayer(id.toString(), message == null ? "" : message);
    }

    @Override
    public void setPlayerListHeader(String header) {
        Native.setPlayerListHeader(id.toString(), header == null ? "" : header);
    }

    @Override
    public void setPlayerListFooter(String footer) {
        Native.setPlayerListFooter(id.toString(), footer == null ? "" : footer);
    }

    @Override
    public void setPlayerListHeaderFooter(String header, String footer) {
        Native.setPlayerListHeaderFooter(id.toString(), header == null ? "" : header,
            footer == null ? "" : footer);
    }

    @Override
    public void sendActionBar(String message) {
        Native.sendActionBar(id.toString(), message == null ? "" : message);
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
        return FotonMessenger.listening(id);
    }

    @Override
    public void sendPluginMessage(Plugin source, String channel, byte[] message) {
        FotonMessenger.send(this, source, channel, message);
    }

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
