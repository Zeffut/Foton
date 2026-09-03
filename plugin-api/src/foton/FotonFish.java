package foton;

import java.util.UUID;

/** Generic fish handle for vanilla fish without a narrower wrapper. */
public final class FotonFish extends FotonLivingEntity implements org.bukkit.entity.Fish {
    public FotonFish(UUID id) { super(id); }
}
