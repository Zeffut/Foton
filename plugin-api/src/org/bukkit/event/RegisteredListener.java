package org.bukkit.event;

import org.bukkit.plugin.Plugin;

/** One handler, as Bukkit's registry describes it. */
public class RegisteredListener {
    private final Listener listener;
    private final EventPriority priority;
    private final Plugin plugin;
    private final boolean ignoreCancelled;

    public RegisteredListener(
            Listener listener, EventPriority priority, Plugin plugin, boolean ignoreCancelled) {
        this.listener = listener;
        this.priority = priority;
        this.plugin = plugin;
        this.ignoreCancelled = ignoreCancelled;
    }

    public Listener getListener() {
        return listener;
    }

    public EventPriority getPriority() {
        return priority;
    }

    public Plugin getPlugin() {
        return plugin;
    }

    public boolean isIgnoringCancelled() {
        return ignoreCancelled;
    }
}
