package org.bukkit.event.block;

import org.bukkit.block.Block;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired when a player submits text for a sign. */
public class SignChangeEvent extends BlockEvent implements Cancellable {
    private final Player player;
    private final String[] lines;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();

    public SignChangeEvent(Block block, Player player, String[] lines) {
        super(block); this.player = player; this.lines = lines.clone();
    }
    public SignChangeEvent(Block block, Player player, String[] lines, org.bukkit.block.sign.Side side) {
        this(block, player, lines);
    }
    public Player getPlayer() { return player; }
    public String getLine(int index) { return lines[index]; }
    public void setLine(int index, String line) { lines[index] = line == null ? "" : line; }
    public String[] getLines() { return lines.clone(); }
    public org.bukkit.block.sign.Side getSide() { return org.bukkit.block.sign.Side.FRONT; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
