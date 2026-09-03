package foton;

import java.util.UUID;
import org.bukkit.entity.Entity;
import org.bukkit.projectiles.ProjectileSource;

/** Live Bukkit view of a Steel fireball. */
public class FotonFireball extends FotonEntity implements org.bukkit.entity.Fireball {
    public FotonFireball(UUID id) { super(id); }
    @Override public ProjectileSource getShooter() {
        String owner = Native.projectileShooter(getUniqueId().toString());
        try { return owner == null ? null : FotonEntity.handle(UUID.fromString(owner)); }
        catch (IllegalArgumentException error) { return null; }
    }
    @Override public void setShooter(ProjectileSource source) {
        Native.setProjectileShooter(getUniqueId().toString(), source instanceof Entity ? ((Entity) source).getUniqueId().toString() : null);
    }
}
