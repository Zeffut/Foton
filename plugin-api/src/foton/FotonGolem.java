package foton;

import java.util.UUID;

/** Generic golem handle for vanilla golems without a narrower wrapper. */
public final class FotonGolem extends FotonLivingEntity implements org.bukkit.entity.Golem {
    public FotonGolem(UUID id) { super(id); }
}
