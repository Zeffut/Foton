package foton;

import java.util.UUID;

/** Generic animal handle for vanilla animals without a narrower wrapper. */
public final class FotonAnimal extends FotonLivingEntity implements org.bukkit.entity.Animal {
    public FotonAnimal(UUID id) { super(id); }
}
