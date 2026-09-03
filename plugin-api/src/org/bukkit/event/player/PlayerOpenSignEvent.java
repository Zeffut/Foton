package org.bukkit.event.player;

import org.bukkit.block.Sign;
import org.bukkit.block.sign.Side;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired when a player is about to open a sign editor. */
public class PlayerOpenSignEvent extends PlayerEvent implements Cancellable {
    private final Sign sign;
    private final Side side;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();

    public PlayerOpenSignEvent(Player player, Sign sign, Side side) {
        super(player);
        this.sign = sign;
        this.side = side;
    }
    public Sign getSign() { return sign; }
    public Side getSide() { return side; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
