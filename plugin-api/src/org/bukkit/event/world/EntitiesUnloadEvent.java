package org.bukkit.event.world;

import java.util.List;
import org.bukkit.Chunk;
import org.bukkit.entity.Entity;
import org.bukkit.event.HandlerList;

/** The entities of a chunk are about to leave with it. They are still in the
 * world while this runs. */
public class EntitiesUnloadEvent extends ChunkEvent {
    private static final HandlerList HANDLERS = new HandlerList();
    private final List<Entity> entities;

    public EntitiesUnloadEvent(Chunk chunk, List<Entity> entities) {
        super(chunk);
        this.entities = entities;
    }

    public List<Entity> getEntities() { return entities; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
