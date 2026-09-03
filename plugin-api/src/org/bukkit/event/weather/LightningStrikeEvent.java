package org.bukkit.event.weather;
import org.bukkit.World;
import org.bukkit.entity.LightningStrike;
import org.bukkit.event.HandlerList;
import org.bukkit.event.Cancellable;
public class LightningStrikeEvent extends WeatherEvent implements Cancellable {
    public enum Cause { COMMAND, TRIDENT, WEATHER, CUSTOM }
    private final LightningStrike lightning;
    private final Cause cause;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public LightningStrikeEvent(World world, LightningStrike lightning, Cause cause) {
        super(world); this.lightning = lightning; this.cause = cause == null ? Cause.CUSTOM : cause;
    }
    public LightningStrike getLightning() { return lightning; }
    public Cause getCause() { return cause; }
    public boolean isCancelled() { return cancelled; }
    public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
