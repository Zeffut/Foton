package org.bukkit.plugin;

import org.bukkit.event.EventPriority;
import org.bukkit.event.Listener;

/** Bukkit-compatible package for a registered event listener. */
public class RegisteredListener extends org.bukkit.event.RegisteredListener {
    public RegisteredListener(Listener listener, EventPriority priority, Plugin plugin, boolean ignoreCancelled) {
        super(listener, priority, plugin, ignoreCancelled);
    }
    public RegisteredListener(Listener listener, EventExecutor executor, EventPriority priority, Plugin plugin, boolean ignoreCancelled) {
        super(listener, executor, priority, plugin, ignoreCancelled);
    }
}
