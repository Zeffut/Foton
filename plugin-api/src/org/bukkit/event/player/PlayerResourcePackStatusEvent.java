package org.bukkit.event.player;

import java.util.UUID;
import org.bukkit.entity.Player;
import org.bukkit.event.HandlerList;

/** Fired when a client reports what it did with a resource pack.
 *
 * <p>Foton sends no resource packs, so nothing constructs this yet. It exists
 * anyway, and the reason is worth stating rather than leaving as dead weight:
 * a listener class references every event type in its method signatures, so a
 * plugin registering a listener with one `PlayerResourcePackStatusEvent`
 * handler fails to register *all* of its handlers when the class is missing.
 * The absent class costs the plugin its other features; the present one costs
 * only the feature that was never there.
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

    @Override public HandlerList getHandlers() { return HANDLERS; }

    public static HandlerList getHandlerList() { return HANDLERS; }
}
