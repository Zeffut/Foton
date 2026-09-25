package io.papermc.paper.event.player;

import org.bukkit.Location;
import org.bukkit.entity.Player;
import org.bukkit.event.HandlerList;
import org.bukkit.event.player.PlayerEvent;

/** The server is about to refuse a player's move and put them back.
 *
 * Not cancellable: a listener allows the move instead, which skips the check
 * that failed, and may silence the warning the server logs for it. */
public class PlayerFailMoveEvent extends PlayerEvent {
    private static final HandlerList HANDLERS = new HandlerList();
    private final FailReason failReason;
    private final Location from;
    private final Location to;
    private boolean allowed;
    private boolean logWarning;

    public PlayerFailMoveEvent(Player player, FailReason failReason, boolean allowed,
            boolean logWarning, Location from, Location to) {
        super(player);
        this.failReason = failReason;
        this.allowed = allowed;
        this.logWarning = logWarning;
        this.from = from;
        this.to = to;
    }

    public FailReason getFailReason() { return failReason; }
    public Location getFrom() { return from.clone(); }
    public Location getTo() { return to.clone(); }
    public boolean isAllowed() { return allowed; }
    public void setAllowed(boolean allowed) { this.allowed = allowed; }
    public boolean getLogWarning() { return logWarning; }
    public void setLogWarning(boolean logWarning) { this.logWarning = logWarning; }

    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }

    public enum FailReason {
        /** Only with Paper's prevent-moving-into-unloaded-chunks, which Foton
         * does not have; never fired here. */
        MOVED_INTO_UNLOADED_CHUNK,
        MOVED_TOO_QUICKLY,
        MOVED_WRONGLY,
        CLIPPED_INTO_BLOCK
    }
}
