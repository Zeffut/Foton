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
    @Override public void setSize(double size, long seconds) { Native.setWorldBorderLerp(world.getName(), getSize(), size, Math.max(0L, seconds) * 20L); }
    @Override public void changeSize(double oldSize, double newSize, long seconds) { Native.setWorldBorderLerp(world.getName(), oldSize, newSize, Math.max(0L, seconds) * 20L); }
    @Override public void changeSize(double newSize, long seconds) { Native.setWorldBorderLerp(world.getName(), getSize(), newSize, Math.max(0L, seconds) * 20L); }
    @Override public void reset() { Native.resetWorldBorder(world.getName()); }
    @Override public boolean isInside(Location location) {
        if (location == null || location.getWorld() == null || !world.getName().equals(location.getWorld().getName())) return false;
        double[] value = state();
        if (value == null) return false;
        double half = value[2] * 0.5D;
        return location.getX() >= value[0] - half && location.getX() <= value[0] + half
                && location.getZ() >= value[1] - half && location.getZ() <= value[1] + half;
    }
    @Override public int getWarningDistance() { return Native.worldBorderWarningDistance(world.getName()); }
    @Override public void setWarningDistance(int distance) { Native.setWorldBorderWarningDistance(world.getName(), distance); }
    @Override public int getWarningTime() { return Native.worldBorderWarningTime(world.getName()) / 20; }
    @Override public void setWarningTime(int seconds) { Native.setWorldBorderWarningTime(world.getName(), Math.max(0, seconds) * 20); }
    @Override public int getWarningTimeTicks() { return Native.worldBorderWarningTime(world.getName()); }
    @Override public void setWarningTimeTicks(int ticks) { Native.setWorldBorderWarningTime(world.getName(), Math.max(0, ticks)); }
    @Override public double getDamageAmount() { return Native.worldBorderDamageAmount(world.getName()); }
    @Override public void setDamageAmount(double damage) { Native.setWorldBorderDamageAmount(world.getName(), damage); }
    @Override public double getDamageBuffer() { return Native.worldBorderDamageBuffer(world.getName()); }
    @Override public void setDamageBuffer(double distance) { Native.setWorldBorderDamageBuffer(world.getName(), distance); }
}
