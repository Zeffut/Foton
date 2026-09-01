package org.bukkit.event;

/** An event that can be stopped before it takes effect. */
public interface Cancellable {
    boolean isCancelled();
    void setCancelled(boolean cancelled);
}
