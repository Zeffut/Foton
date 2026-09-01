package org.bukkit.event.player;

import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;

/** What a player said, before anyone hears it.
 *
 * The name keeps Bukkit's `Async` because that is what plugins import, even
 * though Foton dispatches it from the packet path rather than off-thread.
 */
public class AsyncPlayerChatEvent extends PlayerEvent implements Cancellable {
    private String message;
    private boolean cancelled;

    public AsyncPlayerChatEvent(Player player, String message) {
        super(player);
        this.message = message;
    }

    public String getMessage() { return message; }
    public void setMessage(String value) { this.message = value; }

    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { this.cancelled = value; }
}
