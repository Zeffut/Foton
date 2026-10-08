package org.bukkit.event.player;

import java.util.UUID;
import org.bukkit.entity.Player;
import org.bukkit.event.HandlerList;

/** Fired when a client reports what it did with a resource pack.
 *
 * <p>This is one of the exchanges a pack push starts: the client accepts it,
 * downloads it, then loads it or fails to, and each report arrives here.
 * Reports made while the player was still configuring, which is where the
 * server's own pack is answered, are delivered right after the player joins.
 */
public class PlayerResourcePackStatusEvent extends PlayerEvent {
    /** What the client did with the pack.
     *
     * <p>The eight values of vanilla's `ServerboundResourcePackPacket.Action`,
     * in its order. {@link #isTerminal()} mirrors vanilla too: `ACCEPTED` and
     * `DOWNLOADED` are progress reports, and everything else ends the exchange.
     */
    public enum Status {
        SUCCESSFULLY_LOADED,
        DECLINED,
        FAILED_DOWNLOAD,
        ACCEPTED,
        DOWNLOADED,
        INVALID_URL,
        FAILED_RELOAD,
        DISCARDED;

        public boolean isTerminal() {
            return this != ACCEPTED && this != DOWNLOADED;
        }
    }

    private static final HandlerList HANDLERS = new HandlerList();
    private final UUID id;
    private final Status status;

    public PlayerResourcePackStatusEvent(Player player, UUID id, Status status) {
        super(player);
        this.id = id;
        this.status = status;
    }

    /** Which pack this is about, when several were sent. */
    public UUID getID() {
        return id;
    }

    public Status getStatus() {
        return status;
    }

    /** Always null: the client no longer sends the hash back. */
    @Deprecated(forRemoval = true)
    public String getHash() {
        return null;
    }

    @Override public HandlerList getHandlers() { return HANDLERS; }

    public static HandlerList getHandlerList() { return HANDLERS; }
}
