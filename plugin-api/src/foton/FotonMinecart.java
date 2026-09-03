package foton;

import java.util.UUID;

/** Generic minecart handle backed by Steel entity state. */
public class FotonMinecart extends FotonVehicle implements org.bukkit.entity.Minecart {
    public FotonMinecart(UUID id) { super(id); }
}
