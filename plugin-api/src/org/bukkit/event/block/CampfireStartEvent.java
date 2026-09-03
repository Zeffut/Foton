package org.bukkit.event.block;

import org.bukkit.block.Block;
import org.bukkit.entity.Entity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.inventory.ItemStack;

/** Fired when an item starts cooking on a campfire. */
public class CampfireStartEvent extends BlockEvent implements Cancellable {
    private final ItemStack source; private final Entity entity; private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public CampfireStartEvent(Block block, ItemStack source, Entity entity) { super(block); this.source = source; this.entity = entity; }
    public CampfireStartEvent(Block block, ItemStack source) { this(block, source, null); }
    public ItemStack getSource() { return source; }
    public Entity getEntity() { return entity; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
