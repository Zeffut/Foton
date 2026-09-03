package foton;

import java.util.UUID;

/** Flying hostile handle for vanilla flying monsters without a specialized wrapper. */
public final class FotonFlyingMonster extends FotonMonster implements org.bukkit.entity.Flying {
    public FotonFlyingMonster(UUID id) { super(id); }
}
