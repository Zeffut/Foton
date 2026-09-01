package foton;

import java.util.UUID;
import org.bukkit.entity.LivingEntity;

/** Generic living-entity handle. */
final class FotonLivingEntity extends FotonEntity implements LivingEntity {
    FotonLivingEntity(UUID id) { super(id); }
    @Override public double getHealth() { return Native.health(getUniqueId().toString()); }
    @Override public void setHealth(double value) { Native.setHealth(getUniqueId().toString(), value); }
    @Override public double getMaxHealth() { return Native.maxHealth(getUniqueId().toString()); }
}
