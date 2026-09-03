package org.bukkit.entity;
import java.util.UUID;
import org.bukkit.projectiles.ProjectileSource;
/** Shared projectile ownership API for arrows. */
public interface AbstractArrow extends Projectile {
    @Override default ProjectileSource getShooter() {
        String owner = foton.Native.entityOwner(((foton.FotonEntity) this).getUniqueId().toString());
        try { return owner == null ? null : new foton.FotonEntity(UUID.fromString(owner)); }
        catch (IllegalArgumentException ignored) { return null; }
    }
    @Override default void setShooter(ProjectileSource source) {
        if (source instanceof org.bukkit.entity.Entity entity) foton.Native.setEntityOwner(((foton.FotonEntity) this).getUniqueId().toString(), entity.getUniqueId().toString());
    }
}
