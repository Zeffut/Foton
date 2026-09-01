package org.bukkit.event;

/** How Bukkit calls one handler when the handler was not annotated.
 *
 * A plugin that builds its listeners at runtime hands one of these to
 * `PluginManager#registerEvent`. It is the callable itself, so nothing here
 * has to guess which method was meant.
 */
public interface EventExecutor {
    void execute(Listener listener, Event event) throws EventException;
}
