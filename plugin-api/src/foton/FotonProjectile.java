package foton;

import java.util.UUID;
import org.bukkit.entity.Projectile;
import org.bukkit.projectiles.ProjectileSource;

/** Projectile handle backed by Steel's persisted projectile owner UUID. */
public class FotonProjectile extends FotonEntity implements Projectile {
    public FotonProjectile(UUID id) { super(id); }

    @Override public ProjectileSource getShooter() {
        ProjectileSource retained = Native.entityProjectileSource(getUniqueId().toString());
        if (retained != null) return retained;
        String owner = Native.entityProjectileOwner(getUniqueId().toString());
        if (owner == null) return null;
        try {
            UUID id = UUID.fromString(owner);
            String type = Native.entityType(owner);
            org.bukkit.entity.Entity entity = FotonWorld.wrapEntity(id, type);
            return entity instanceof ProjectileSource source ? source : null;
        } catch (IllegalArgumentException error) {
            return null;
        }
    }

    @Override public void setShooter(ProjectileSource source) {
        setProjectileShooter(source, false);
    }

    protected final void setProjectileShooter(ProjectileSource source, boolean resetPickupStatus) {
        String owner = source instanceof org.bukkit.entity.Entity entity
            ? entity.getUniqueId().toString() : null;
        Native.setEntityProjectileSource(getUniqueId().toString(), owner == null ? "" : owner,
            source, resetPickupStatus);
    }
}
