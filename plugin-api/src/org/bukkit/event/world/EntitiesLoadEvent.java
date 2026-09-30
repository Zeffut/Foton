package org.bukkit.event.world;

import java.util.List;
import org.bukkit.Chunk;
import org.bukkit.entity.Entity;
import org.bukkit.event.HandlerList;

/** Entities saved in a chunk came back into the world with it. */
public class EntitiesLoadEvent extends ChunkEvent {
    private static final HandlerList HANDLERS = new HandlerList();
    private final List<Entity> entities;

    public EntitiesLoadEvent(Chunk chunk, List<Entity> entities) {
        super(chunk);
        this.entities = entities;
    }

    public List<Entity> getEntities() { return entities; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
