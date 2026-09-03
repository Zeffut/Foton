package org.bukkit.event.world;

import org.bukkit.World;
import org.bukkit.event.Event;

/** Base for events associated with one world. */
public abstract class WorldEvent extends Event {
    private final World world;
    protected WorldEvent(World world) { this.world = world; }
    public World getWorld() { return world; }
}
