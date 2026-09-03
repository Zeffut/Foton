package foton;

import java.util.UUID;

/** Hopper minecart backed by Steel's HopperMinecartEntity. */
public final class FotonHopperMinecart extends FotonMinecart implements org.bukkit.entity.minecart.HopperMinecart {
    public FotonHopperMinecart(UUID id) { super(id); }
}
