package org.bukkit.entity;

import org.bukkit.projectiles.ProjectileSource;

/** An entity that was launched by another source. */
public interface Projectile extends Entity {
    ProjectileSource getShooter();
    void setShooter(ProjectileSource source);
}
