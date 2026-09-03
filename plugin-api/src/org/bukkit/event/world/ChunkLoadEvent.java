package org.bukkit.event.world;

import org.bukkit.Chunk;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

public final class ChunkLoadEvent extends Event {
    private final Chunk chunk; private final boolean newChunk;
    private static final HandlerList HANDLERS = new HandlerList();
    public ChunkLoadEvent(Chunk chunk, boolean newChunk) { this.chunk = chunk; this.newChunk = newChunk; }
    public Chunk getChunk() { return chunk; }
    public org.bukkit.World getWorld() { return chunk == null ? null : chunk.getWorld(); }
    public boolean isNewChunk() { return newChunk; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
