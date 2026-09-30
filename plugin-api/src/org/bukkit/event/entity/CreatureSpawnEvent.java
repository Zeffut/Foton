package org.bukkit.event.entity;

import org.bukkit.Location;
import org.bukkit.entity.LivingEntity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired before a creature is inserted into a world.
 *
 * <p>Extends {@link EntitySpawnEvent}, which is Bukkit's hierarchy and not a
 * detail: a plugin listening for every spawn registers for the base class, and
 * creatures are most of what spawns. Parenting this to `EntityEvent` instead
 * left those listeners silent for the case they were written for.
 */
public class CreatureSpawnEvent extends EntitySpawnEvent implements Cancellable {
    public enum SpawnReason {
        NATURAL, JOCKEY,
        @Deprecated(since = "1.14", forRemoval = true) CHUNK_GEN,
        SPAWNER, TRIAL_SPAWNER, EGG, SPAWNER_EGG, LIGHTNING, BUILD_SNOWMAN,
        BUILD_IRONGOLEM, BUILD_COPPERGOLEM, BUILD_WITHER, VILLAGE_DEFENSE,
        VILLAGE_INVASION, BREEDING, SLIME_SPLIT, REINFORCEMENTS, NETHER_PORTAL,
        DISPENSE_EGG, INFECTION, CURED, OCELOT_BABY, SILVERFISH_BLOCK, MOUNT, TRAP,
        ENDER_PEARL, SHOULDER_ENTITY, DROWNED, SHEARED, EXPLOSION, RAID, PATROL,
        BEEHIVE, PIGLIN_ZOMBIFIED, SPELL, FROZEN, METAMORPHOSIS, DUPLICATION,
        COMMAND, ENCHANTMENT, OMINOUS_ITEM_SPAWNER, BUCKET, POTION_EFFECT,
        REANIMATE, REHYDRATION, CUSTOM, DEFAULT
    }
    private final Location location;
    private final SpawnReason reason;
    private static final HandlerList HANDLERS = new HandlerList();

    public CreatureSpawnEvent(LivingEntity entity, Location location, SpawnReason reason) {
        super(entity); this.location = location; this.reason = reason;
    }
    @Override public LivingEntity getEntity() { return (LivingEntity) super.getEntity(); }
    /** The spawn point, which is where the creature was asked to appear rather
     * than where it ended up -- the two differ when placement nudges it. */
    @Override public Location getLocation() { return location; }
    public SpawnReason getSpawnReason() { return reason; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
