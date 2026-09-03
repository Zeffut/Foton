package org.bukkit.event;

/** Something that happened, which handlers may see.
 *
 * `getHandlers` is abstract in Bukkit and every event declares it alongside a
 * static `getHandlerList`. Plugins define their own events the same way, so
 * the shape has to be here even though Foton dispatches through
 * `foton.EventBridge` rather than through these lists.
 */
public abstract class Event {
    /** Result used by events that distinguish allow, default, and deny. */
    public enum Result { DENY, DEFAULT, ALLOW }

    private final boolean async;

    protected Event() {
        this(false);
    }

    protected Event(boolean async) {
        this.async = async;
    }

    public String getEventName() {
        return getClass().getSimpleName();
    }

    /** Whether this was fired off the main thread. */
    public boolean isAsynchronous() {
        return async;
    }

    public abstract HandlerList getHandlers();
}
