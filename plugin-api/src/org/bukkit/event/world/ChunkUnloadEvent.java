package org.bukkit.event.world;

import java.util.Objects;
import org.bukkit.Chunk;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired as a chunk is unloaded.
 *
 * <p>Not cancellable, which is modern Bukkit's shape and the honest one here:
 * by the time this fires the unload is under way, and a `setCancelled` that
 * changed nothing would be worse than its absence -- a plugin would believe it
 * had kept the chunk.
 */
public class ChunkUnloadEvent extends Event {
    private static final HandlerList HANDLERS = new HandlerList();
    private final Chunk chunk;
    private boolean save;

    public ChunkUnloadEvent(Chunk chunk) { this(chunk, true); }

    public ChunkUnloadEvent(Chunk chunk, boolean save) {
        this.chunk = Objects.requireNonNull(chunk, "chunk");
        this.save = save;
    }

    public Chunk getChunk() { return chunk; }

    public org.bukkit.World getWorld() { return chunk.getWorld(); }

    public boolean isSaveChunk() { return save; }

    public void setSaveChunk(boolean save) { this.save = save; }

    @Override public HandlerList getHandlers() { return HANDLERS; }

    public static HandlerList getHandlerList() { return HANDLERS; }
}
