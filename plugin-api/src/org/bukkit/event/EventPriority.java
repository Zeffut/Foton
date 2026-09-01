package org.bukkit.event;

/** When a handler runs, relative to the others.
 *
 * LOWEST runs first and HIGHEST last, so that a handler at HIGHEST has the
 * final word. MONITOR runs after all of them to observe the outcome and must
 * not change it.
 */
public enum EventPriority {
    LOWEST,
    LOW,
    NORMAL,
    HIGH,
    HIGHEST,
    MONITOR
}
