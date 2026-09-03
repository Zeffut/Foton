package org.bukkit.event.world;

import java.util.Objects;
import org.bukkit.Chunk;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired before a chunk is unloaded. */
public class ChunkUnloadEvent extends Event implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final Chunk chunk;
    private boolean save;
    private boolean cancelled;

    public ChunkUnloadEvent(Chunk chunk) { this(chunk, true); }
    public ChunkUnloadEvent(Chunk chunk, boolean save) {
        this.chunk = Objects.requireNonNull(chunk, "chunk");
        this.save = save;
    }
    public Chunk getChunk() { return chunk; }
    public org.bukkit.World getWorld() { return chunk.getWorld(); }
    public boolean isSaveChunk() { return save; }
    public void setSaveChunk(boolean save) { this.save = save; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
