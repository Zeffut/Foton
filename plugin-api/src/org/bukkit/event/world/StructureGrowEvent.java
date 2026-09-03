package org.bukkit.event.world;

import java.util.List;
import org.bukkit.Location;
import org.bukkit.block.BlockState;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired before a natural tree or similar structure is placed. */
public class StructureGrowEvent extends Event implements Cancellable {
    private final Location location;
    private final Player player;
    private final List<BlockState> blocks;
    private final org.bukkit.TreeType species;
    private final boolean fromBonemeal;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public StructureGrowEvent(Location location, Player player, List<BlockState> blocks) {
        this(location, player, blocks, org.bukkit.TreeType.TREE, player != null);
    }
    public StructureGrowEvent(Location location, Player player, List<BlockState> blocks, org.bukkit.TreeType species, boolean fromBonemeal) {
        this.location = location; this.player = player; this.blocks = blocks; this.species = species; this.fromBonemeal = fromBonemeal;
    }
    public Location getLocation() { return location; }
    public org.bukkit.World getWorld() { return location == null ? null : location.getWorld(); }
    public Player getPlayer() { return player; }
    public List<BlockState> getBlocks() { return blocks; }
    public org.bukkit.TreeType getSpecies() { return species; }
    public boolean isFromBonemeal() { return fromBonemeal; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
