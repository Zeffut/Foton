package org.bukkit.event.entity;

import com.google.common.base.Function;
import java.util.Map;
import org.bukkit.block.Block;
import org.bukkit.block.BlockState;
import org.bukkit.entity.Entity;

/** Damage caused by a block. The block may be unknown, as Paper's may. */
public class EntityDamageByBlockEvent extends EntityDamageEvent {
    private final Block damager;
    private final BlockState damagerState;
    public EntityDamageByBlockEvent(Block damager, Entity entity, DamageCause cause) {
        this(damager, entity, cause, 0.0);
    }
    public EntityDamageByBlockEvent(Block damager, Entity entity, DamageCause cause, double damage) {
        this(damager, null, entity, cause, null, damage);
    }
    public EntityDamageByBlockEvent(Block damager, BlockState damagerState, Entity entity, DamageCause cause,
            org.bukkit.damage.DamageSource source, double damage) {
        super(entity, cause, source, damage);
        this.damager = damager;
        this.damagerState = damagerState;
    }
    public EntityDamageByBlockEvent(Block damager, Entity entity, DamageCause cause, Map<DamageModifier, Double> modifiers,
            Map<DamageModifier, ? extends Function<? super Double, Double>> modifierFunctions) {
        this(damager, null, entity, cause, null, modifiers, modifierFunctions);
    }
    public EntityDamageByBlockEvent(Block damager, BlockState damagerState, Entity entity, DamageCause cause,
            org.bukkit.damage.DamageSource source, Map<DamageModifier, Double> modifiers,
            Map<DamageModifier, ? extends Function<? super Double, Double>> modifierFunctions) {
        super(entity, cause, source, modifiers, modifierFunctions);
        this.damager = damager;
        this.damagerState = damagerState;
    }
    public Block getDamager() { return damager; }
    public BlockState getDamagerBlockState() { return damagerState; }
}
