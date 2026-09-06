package org.spigotmc;

/** Spigot's watchdog, as plugins reach for it.
 *
 * <p>Long-running plugin work calls {@link #tick()} to say the server is alive
 * and stop the watchdog killing it mid-task. Foton has no watchdog, so the tick
 * has nothing to reset -- but a plugin that cannot call it fails to load at all,
 * and one that calls it here simply proceeds, which is the outcome it wanted.
 */
public final class WatchdogThread extends Thread {
    private static final WatchdogThread INSTANCE = new WatchdogThread();

    private WatchdogThread() {
        super("Foton Watchdog Thread");
        setDaemon(true);
    }

    /** Marks the server as alive. A no-op here: nothing is watching. */
    public static void tick() { }

    /** Spigot's own accessor, kept so plugins that reach for the thread link. */
    public static WatchdogThread getInstance() { return INSTANCE; }

    public static void doStart(int timeoutSeconds, boolean restart) { }
}
