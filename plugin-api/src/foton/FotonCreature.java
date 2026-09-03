package foton;

import java.util.UUID;

/** Generic mob handle for living entity types without a narrower wrapper. */
public final class FotonCreature extends FotonLivingEntity implements org.bukkit.entity.Creature {
    public FotonCreature(UUID id) { super(id); }
}
