package foton;

import java.util.UUID;
import org.bukkit.entity.Projectile;
import org.bukkit.projectiles.ProjectileSource;

/** Projectile handle backed by Steel's persisted projectile owner UUID. */
public class FotonProjectile extends FotonEntity implements Projectile {
    public FotonProjectile(UUID id) { super(id); }

    @Override public ProjectileSource getShooter() {
        String owner = Native.entityProjectileOwner(getUniqueId().toString());
        if (owner == null) return null;
        try {
            UUID id = UUID.fromString(owner);
            String type = Native.entityType(owner);
            return "player".equalsIgnoreCase(type) ? new FotonPlayer(id) : FotonEntity.handle(id);
        } catch (IllegalArgumentException error) {
            return null;
        }
    }

    @Override public void setShooter(ProjectileSource source) {
        String owner = source instanceof org.bukkit.entity.Entity entity
            ? entity.getUniqueId().toString() : null;
        Native.setEntityProjectileOwner(getUniqueId().toString(), owner == null ? "" : owner);
    }
}
