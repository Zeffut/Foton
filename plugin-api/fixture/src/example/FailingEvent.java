package example;

/** A plugin-defined event used to prove failed lifecycle listeners are released. */
public final class FailingEvent extends org.bukkit.event.Event {
    private static final org.bukkit.event.HandlerList HANDLERS =
        new org.bukkit.event.HandlerList();

    @Override
    public org.bukkit.event.HandlerList getHandlers() {
        return HANDLERS;
    }

    public static org.bukkit.event.HandlerList getHandlerList() {
        return HANDLERS;
    }
}
