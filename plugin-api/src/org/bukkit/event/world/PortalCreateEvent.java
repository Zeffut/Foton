package org.bukkit.event.world;

import java.util.List;
import org.bukkit.World;
import org.bukkit.block.BlockState;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;
import org.bukkit.entity.Entity;

public final class PortalCreateEvent extends Event {
    public enum CreateReason { NETHER_PAIR, END_PLATFORM, FIRE, OBC, OTHER }
    private final List<BlockState> blocks; private final World world; private boolean cancelled;
    private final CreateReason reason;
    private final Entity entity;
    private static final HandlerList HANDLERS = new HandlerList();
    public PortalCreateEvent(World world, List<BlockState> blocks) { this(world, blocks, CreateReason.OTHER); }
    public PortalCreateEvent(World world, List<BlockState> blocks, CreateReason reason) { this(world, blocks, reason, null); }
    public PortalCreateEvent(World world, List<BlockState> blocks, CreateReason reason, Entity entity) { this.world = world; this.blocks = blocks; this.reason = reason == null ? CreateReason.OTHER : reason; this.entity = entity; }
    public List<BlockState> getBlocks() { return blocks; }
    public World getWorld() { return world; }
    public CreateReason getReason() { return reason; }
    public Entity getEntity() { return entity; }
    public boolean isCancelled() { return cancelled; }
    public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
