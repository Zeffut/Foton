package org.bukkit.plugin;

/** Which provider wins when several plugins publish the same service. */
public enum ServicePriority {
    Lowest,
    Low,
    Normal,
    High,
    Highest
}
