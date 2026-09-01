package org.bukkit.plugin;

import org.bukkit.event.Event;
import org.bukkit.event.EventException;
import org.bukkit.event.Listener;

/** The callable behind a handler registered without an annotation. */
public interface EventExecutor {
    void execute(Listener listener, Event event) throws EventException;
}
