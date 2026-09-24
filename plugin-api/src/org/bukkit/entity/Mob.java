package org.bukkit.entity;

/** A living entity with a target. */
public interface Mob extends LivingEntity {
    LivingEntity getTarget();
    /** Whether the mob runs its goals and brain; an unaware mob still falls and can be pushed. */
    default boolean isAware() { return foton.Native.mobAware(getUniqueId().toString()); }
    default void setAware(boolean aware) { foton.Native.setMobAware(getUniqueId().toString(), aware); }
    void setTarget(LivingEntity target);
}
