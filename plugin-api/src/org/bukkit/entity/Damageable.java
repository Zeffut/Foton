package org.bukkit.entity;
public interface Damageable extends Entity {
    /** Hurts the entity through the game's own damage pipeline, as generic damage. */
    default void damage(double amount) { damage(amount, null); }

    /** Hurts the entity as an attack by {@code source}: a player's attack when
     * it is a player, a mob's when it is any other living entity. */
    default void damage(double amount, Entity source) {
        foton.Native.damageEntity(getUniqueId().toString(), amount, source == null ? null : source.getUniqueId().toString());
    }

    double getHealth();
    void setHealth(double health);
    double getMaxHealth();
    default void setMaxHealth(double health) { }
}
