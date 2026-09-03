package foton;

import java.util.UUID;

/** Bukkit vehicle handle backed by Steel's generic entity state. */
public class FotonVehicle extends FotonEntity implements org.bukkit.entity.Vehicle {
    public FotonVehicle(UUID id) { super(id); }
}
