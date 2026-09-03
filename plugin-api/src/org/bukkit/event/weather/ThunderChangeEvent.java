package org.bukkit.event.weather;

import org.bukkit.World;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.event.world.WorldEvent;

/** Fired when thunder starts or stops. */
public class ThunderChangeEvent extends WorldEvent implements Cancellable {
    private final boolean thunder;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public ThunderChangeEvent(World world, boolean thunder) { super(world); this.thunder = thunder; }
    public boolean toThunderState() { return thunder; }
    public boolean isThundering() { return thunder; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
