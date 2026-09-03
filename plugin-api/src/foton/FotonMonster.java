package foton;

import java.util.UUID;

/** Generic hostile-mob handle for hostile types without a narrower API wrapper. */
public class FotonMonster extends FotonLivingEntity implements org.bukkit.entity.Monster {
    public FotonMonster(UUID id) { super(id); }
}
