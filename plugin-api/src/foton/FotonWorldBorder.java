package foton;

import org.bukkit.Location;
import org.bukkit.WorldBorder;

public final class FotonWorldBorder implements WorldBorder {
    private final FotonWorld world;
    public FotonWorldBorder(FotonWorld world) { this.world = world; }
    private double[] state() { return Native.worldBorder(world.getName()); }
    @Override public Location getCenter() {
        double[] value = state();
        return value == null ? new Location(world, 0, 0, 0) : new Location(world, value[0], 0, value[1]);
    }
    @Override public void setCenter(double x, double z) { Native.setWorldBorderCenter(world.getName(), x, z); }
    @Override public double getSize() {
        double[] value = state();
        return value == null ? 5.9999968E7 : value[2];
    }
    @Override public void setSize(double size) { Native.setWorldBorderSize(world.getName(), size); }
}
