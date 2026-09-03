package org.bukkit.event.world;

import org.bukkit.Chunk;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired after a chunk is populated. */
public class ChunkPopulateEvent extends Event {
    private final Chunk chunk;
    private static final HandlerList HANDLERS = new HandlerList();
    public ChunkPopulateEvent(Chunk chunk) { this.chunk = chunk; }
    public Chunk getChunk() { return chunk; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
