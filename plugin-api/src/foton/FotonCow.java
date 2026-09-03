package foton;

import java.util.UUID;

/** Cow entity handle backed by Steel's living-entity state. */
public final class FotonCow extends FotonLivingEntity implements org.bukkit.entity.Cow {
    public FotonCow(UUID id) { super(id); }
}
