package foton;

import java.util.UUID;

/** Generic flying-entity handle for types without a narrower wrapper. */
public class FotonFlying extends FotonLivingEntity implements org.bukkit.entity.Flying {
    public FotonFlying(UUID id) { super(id); }
}
