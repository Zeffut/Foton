package org.bukkit.event;

import java.util.ArrayList;
import java.util.List;
import org.bukkit.plugin.Plugin;

/** Bukkit's per-event handler registry.
 *
 * Every Bukkit event carries a static one, and a plugin defining its own event
 * has to as well -- that is what `getHandlerList` is for and why sixteen of
 * the fifty-nine plugins surveyed construct one.
 *
 * Foton does not dispatch through these. `foton.EventBridge` reflects over
 * `@EventHandler` methods and holds the one registry that matters, so this
 * exists to be the shape plugins compile against, and `unregisterAll` reaches
 * into that registry rather than keeping a second one that would disagree.
 */
public class HandlerList {
    private static final List<HandlerList> ALL = new ArrayList<>();
    private final List<org.bukkit.plugin.RegisteredListener> listeners = new ArrayList<>();

    // The list registers itself so `getHandlerLists` can find it, which
    // means `this` escapes before a subclass finishes constructing. Nothing
    // reads a subclass's state through it -- the collection is only ever
    // walked to unregister -- and Bukkit's own does exactly this, so a plugin
    // that subclasses HandlerList is not surprised by it here.
    @SuppressWarnings("this-escape")
    public HandlerList() {
        synchronized (ALL) {
            ALL.add(this);
        }
    }

    public synchronized void register(org.bukkit.plugin.RegisteredListener listener) {
        if (listener != null && !listeners.contains(listener)) listeners.add(listener);
    }

    public synchronized void unregister(org.bukkit.plugin.RegisteredListener listener) { listeners.remove(listener); }

    public synchronized void unregister(Listener listener) { listeners.removeIf(value -> value.getListener() == listener); }

    public void unregister(Plugin plugin) {
        synchronized (this) { listeners.removeIf(value -> value.getPlugin() == plugin); }
        foton.EventBridge.unregister(plugin);
    }

    public synchronized org.bukkit.plugin.RegisteredListener[] getRegisteredListeners() {
        return listeners.toArray(new org.bukkit.plugin.RegisteredListener[0]);
    }

    public static void unregisterAll() {
        foton.EventBridge.unregisterAll();
    }

    public static void unregisterAll(Plugin plugin) {
        foton.EventBridge.unregister(plugin);
    }

    public static void unregisterAll(Listener listener) {
        foton.EventBridge.unregister(listener);
    }

    public static ArrayList<HandlerList> getHandlerLists() {
        synchronized (ALL) {
            return new ArrayList<>(ALL);
        }
    }
}
