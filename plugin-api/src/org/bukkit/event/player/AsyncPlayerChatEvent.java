package org.bukkit.event.player;

import org.bukkit.event.HandlerList;

import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;

/** What a player said, before anyone hears it.
 *
 * The name keeps Bukkit's `Async` because that is what plugins import, even
 * though Foton dispatches it from the packet path rather than off-thread.
 */
public class AsyncPlayerChatEvent extends PlayerEvent implements Cancellable {
    private String message;
    private final java.util.Set<Player> recipients = new java.util.LinkedHashSet<>();
    private boolean cancelled;
    private String format = "<%1$s> %2$s";

    public AsyncPlayerChatEvent(Player player, String message) {
        super(player);
        this.message = message;
    }

    public String getFormat() { return format; }
    public void setFormat(String value) { format = value == null ? "" : value; }

    public String getMessage() { return message; }
    public void setMessage(String value) { this.message = value; }
    public java.util.Set<Player> getRecipients() { return recipients; }

    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { this.cancelled = value; }

    /** Bukkit gives every event its own handler list, and plugins reach for
     * the static one to register or unregister by hand. Foton dispatches
     * through foton.EventBridge instead, so this is the shape rather than the
     * mechanism -- but a plugin that cannot find it does not compile. */
    private static final HandlerList HANDLERS = new HandlerList();

    @Override
    public HandlerList getHandlers() {
        return HANDLERS;
    }

    public static HandlerList getHandlerList() {
        return HANDLERS;
    }
}
