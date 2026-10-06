package org.bukkit.event.entity;

import com.google.common.base.Function;
import com.google.common.base.Functions;
import java.util.EnumMap;
import java.util.Map;
import org.bukkit.entity.Entity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Base event for damage applied to an entity.
 *
 * Paper's model: the raw amount is the {@code BASE} modifier, every reduction
 * is a negative modifier, and the final damage is their sum. Changing the raw
 * amount runs each modifier's function again on what is left of it.
 */
public class EntityDamageEvent extends EntityEvent implements Cancellable {
    /** Paper's causes, in Paper's order. */
    public enum DamageCause {
        KILL, WORLD_BORDER, CONTACT, ENTITY_ATTACK, ENTITY_SWEEP_ATTACK, PROJECTILE, SUFFOCATION,
        FALL, FIRE, FIRE_TICK, MELTING, LAVA, DROWNING, BLOCK_EXPLOSION, ENTITY_EXPLOSION, VOID,
        LIGHTNING, SUICIDE, STARVATION, POISON, MAGIC, WITHER, FALLING_BLOCK, THORNS,
        DRAGON_BREATH, FLY_INTO_WALL, HOT_FLOOR, CAMPFIRE, CRAMMING, DRYOUT, FREEZE, SONIC_BOOM,
        CUSTOM
    }

    /** Paper's modifiers, in the order the server applies them. */
    @Deprecated(since = "1.12")
    public enum DamageModifier {
        BASE, INVULNERABILITY_REDUCTION, FREEZING, HARD_HAT, BLOCKING, ARMOR, RESISTANCE, MAGIC,
        ABSORPTION
    }

    private static final DamageModifier[] MODIFIERS = DamageModifier.values();
    private static final Function<? super Double, Double> ZERO = Functions.constant(-0.0);
    private static final HandlerList HANDLERS = new HandlerList();
    private final Map<DamageModifier, Double> modifiers;
    private final Map<DamageModifier, ? extends Function<? super Double, Double>> modifierFunctions;
    private final Map<DamageModifier, Double> originals;
    private final DamageCause cause;
    private final org.bukkit.damage.DamageSource damageSource;
    private boolean cancelled;

    protected EntityDamageEvent(Entity entity) { this(entity, DamageCause.CUSTOM); }
    protected EntityDamageEvent(Entity entity, DamageCause cause) { this(entity, cause, 0.0); }
    public EntityDamageEvent(Entity entity, DamageCause cause, double damage) {
        this(entity, cause, null, damage);
    }
    public EntityDamageEvent(Entity entity, DamageCause cause, org.bukkit.damage.DamageSource source, double damage) {
        this(entity, cause, source, baseOnly(damage), baseFunction());
    }
    public EntityDamageEvent(Entity entity, DamageCause cause, Map<DamageModifier, Double> modifiers,
            Map<DamageModifier, ? extends Function<? super Double, Double>> modifierFunctions) {
        this(entity, cause, null, modifiers, modifierFunctions);
    }
    public EntityDamageEvent(Entity entity, DamageCause cause, org.bukkit.damage.DamageSource source,
            Map<DamageModifier, Double> modifiers,
            Map<DamageModifier, ? extends Function<? super Double, Double>> modifierFunctions) {
        super(entity);
        if (!modifiers.containsKey(DamageModifier.BASE)) throw new IllegalArgumentException("BASE DamageModifier missing");
        if (!modifiers.keySet().equals(modifierFunctions.keySet())) {
            throw new IllegalArgumentException("Must have a modifier function for each DamageModifier");
        }
        this.cause = cause == null ? DamageCause.CUSTOM : cause;
        this.damageSource = source;
        this.modifiers = modifiers;
        this.modifierFunctions = modifierFunctions;
        this.originals = new EnumMap<>(modifiers);
    }

    static Map<DamageModifier, Double> baseOnly(double damage) {
        Map<DamageModifier, Double> map = new EnumMap<>(DamageModifier.class);
        map.put(DamageModifier.BASE, damage);
        return map;
    }

    static Map<DamageModifier, Function<? super Double, Double>> baseFunction() {
        Map<DamageModifier, Function<? super Double, Double>> map = new EnumMap<>(DamageModifier.class);
        map.put(DamageModifier.BASE, ZERO);
        return map;
    }

    public org.bukkit.entity.EntityType getEntityType() { return getEntity() == null ? null : getEntity().getType(); }
    public DamageCause getCause() { return cause; }
    public org.bukkit.damage.DamageSource getDamageSource() { return damageSource; }

    public double getOriginalDamage(DamageModifier type) {
        if (type == null) throw new IllegalArgumentException("Cannot have null DamageModifier");
        Double damage = originals.get(type);
        return damage == null ? 0 : damage;
    }

    public void setDamage(DamageModifier type, double damage) {
        if (type == null) throw new IllegalArgumentException("Cannot have null DamageModifier");
        if (!modifiers.containsKey(type)) throw new UnsupportedOperationException(type + " is not applicable to " + getEntity());
        modifiers.put(type, damage);
    }

    public double getDamage(DamageModifier type) {
        if (type == null) throw new IllegalArgumentException("Cannot have null DamageModifier");
        Double damage = modifiers.get(type);
        return damage == null ? 0 : damage;
    }

    public boolean isApplicable(DamageModifier type) {
        if (type == null) throw new IllegalArgumentException("Cannot have null DamageModifier");
        return modifiers.containsKey(type);
    }

    /** The raw amount, before any reduction. */
    public double getDamage() { return getDamage(DamageModifier.BASE); }

    /** What is left of the raw amount once every reduction is taken off. */
    public final double getFinalDamage() {
        double damage = 0;
        for (DamageModifier modifier : MODIFIERS) damage += getDamage(modifier);
        return damage;
    }

    /** Changes the raw amount, and works each reduction out again on it. */
    public void setDamage(double damage) {
        double remaining = damage;
        double oldRemaining = getDamage(DamageModifier.BASE);
        for (DamageModifier modifier : MODIFIERS) {
            if (!isApplicable(modifier)) continue;
            Function<? super Double, Double> function = modifierFunctions.get(modifier);
            double newVanilla = function.apply(remaining);
            double oldVanilla = function.apply(oldRemaining);
            double difference = oldVanilla - newVanilla;
            // A reduction never crosses zero.
            double old = getDamage(modifier);
            if (old > 0) setDamage(modifier, Math.max(0, old - difference));
            else setDamage(modifier, Math.min(0, old - difference));
            remaining += newVanilla;
            oldRemaining += oldVanilla;
        }
        setDamage(DamageModifier.BASE, damage);
    }

    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
