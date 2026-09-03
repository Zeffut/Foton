package org.bukkit.event.entity;

import org.bukkit.block.Block;
import org.bukkit.entity.Entity;

/** Damage caused by a block. */
public class EntityDamageByBlockEvent extends EntityDamageEvent {
    private final Block damager;
    public EntityDamageByBlockEvent(Block damager, Entity entity, DamageCause cause) {
        super(entity, cause);
        this.damager = damager;
    }
    public Block getDamager() { return damager; }
}
