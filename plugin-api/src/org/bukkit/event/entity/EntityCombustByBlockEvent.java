package org.bukkit.event.entity;

import org.bukkit.block.Block;
import org.bukkit.entity.Entity;

/** Called when a block causes an entity to combust. The block may be unknown, as Paper's may. */
public class EntityCombustByBlockEvent extends EntityCombustEvent {
    private final Block combuster;
    public EntityCombustByBlockEvent(Block combuster, Entity combustee, int duration) {
        this(combuster, combustee, (float) duration);
    }
    public EntityCombustByBlockEvent(Block combuster, Entity combustee, float duration) {
        super(combustee, duration);
        this.combuster = combuster;
    }
    public Block getCombuster() { return combuster; }
}
