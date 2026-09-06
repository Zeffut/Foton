package io.papermc.paper.event.player;

import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.event.player.PlayerEvent;

/** Fired when a client reports it has finished loading the world it was sent to.
 *
 * <p>The moment vanilla's `ServerboundPlayerLoadedPacket` arrives, which is the
 * first point at which the client is genuinely in the world rather than looking
 * at a loading screen. Plugins use it to stop sending things nobody can see yet.
 *
 * <p>Cancelling suppresses the server's own response to the packet; it does not
 * un-load the client, which has already loaded.
 */
public class PlayerClientLoadedWorldEvent extends PlayerEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final boolean fromNetworking;
    private boolean cancelled;

    public PlayerClientLoadedWorldEvent(Player player, boolean fromNetworking) {
        super(player);
        this.fromNetworking = fromNetworking;
    }

    /** Whether the client said so, rather than the server assuming it.
     *
     * <p>False when the server decided the client must be loaded -- after a
     * respawn or a world change it does not wait to be told. */
    public boolean isFromNetworking() {
        return fromNetworking;
    }

    @Override public boolean isCancelled() { return cancelled; }

    @Override public void setCancelled(boolean cancel) { this.cancelled = cancel; }

    @Override public HandlerList getHandlers() { return HANDLERS; }

    public static HandlerList getHandlerList() { return HANDLERS; }
}
