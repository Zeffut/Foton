package foton;

import org.bukkit.Location;

/** Stable snapshot backed by Bukkit's serializable entity type and position. */
public final class FotonEntitySnapshot implements org.bukkit.entity.EntitySnapshot {
    private final org.bukkit.entity.EntityType type;
    private final Location location;
    public FotonEntitySnapshot(org.bukkit.entity.EntityType type, Location location) {
        this.type = type;
        this.location = location == null ? null : location.clone();
    }
    @Override public org.bukkit.entity.Entity createEntity(Location target) {
        if (target == null || target.getWorld() == null || type == null) return null;
        return target.getWorld().spawnEntity(target, type);
    }
    @Override public String getAsString() {
        if (type == null || location == null || location.getWorld() == null) return "";
        return type.getName() + "@" + location.getWorld().getName() + "," + location.getX() + "," + location.getY() + "," + location.getZ();
    }
}
