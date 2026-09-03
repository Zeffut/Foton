package foton;

import java.util.UUID;
import org.bukkit.OfflinePlayer;
import org.bukkit.entity.Player;

/** Somebody who has played here, whether or not they are here now.
 *
 * Holds only the two things a plugin asks an offline handle for. A name that
 * was never seen online comes back as whatever the plugin asked with, because
 * Bukkit does the same: `getOfflinePlayer("nobody")` answers a handle, not
 * null, and plugins branch on `hasPlayedBefore` rather than on nullness.
 */
public class FotonOfflinePlayer implements OfflinePlayer {
    private final UUID id;
    private final String name;

    FotonOfflinePlayer(UUID id, String name) {
        this.id = id;
        this.name = name;
    }

    @Override
    public UUID getUniqueId() {
        return id;
    }

    @Override
    public String getName() {
        Player online = getPlayer();
        return online == null ? name : online.getName();
    }

    @Override
    public boolean isOnline() {
        return getPlayer() != null;
    }

    @Override
    public Player getPlayer() {
        if (id == null) {
            return null;
        }
        FotonPlayer player = new FotonPlayer(id);
        return player.isOnline() ? player : null;
    }

    @Override public boolean isOp() { return id != null && Native.offlineIsOperator(id.toString()); }

    @Override public void setOp(boolean value) { if (id != null) Native.setPlayerOperator(id.toString(), value); }

    @Override public boolean isWhitelisted() { return id != null && Native.offlineIsWhitelisted(id.toString()); }

    @Override public void setWhitelisted(boolean value) { if (id != null) Native.setPlayerWhitelisted(id.toString(), value); }

    @Override public boolean isBanned() { return name != null && FotonServer.isNameBanned(name); }

    @Override public long getFirstPlayed() { return id == null ? 0L : Native.firstPlayed(id.toString()); }

    @Override public long getLastPlayed() { return id == null ? 0L : Native.lastPlayed(id.toString()); }

    @Override public int getStatistic(org.bukkit.Statistic statistic) {
        return id == null || statistic == null ? 0 : Native.offlineStatistic(id.toString(), statistic.name());
    }

    @Override public boolean hasPlayedBefore() {
        return id != null && Native.hasPlayedBefore(id.toString());
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof FotonOfflinePlayer offline
            && java.util.Objects.equals(id, offline.id);
    }

    @Override
    public int hashCode() {
        return java.util.Objects.hash(id);
    }

    @Override
    public String toString() {
        return "FotonOfflinePlayer{" + name + " " + id + "}";
    }
}
