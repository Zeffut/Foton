package foton;

import java.util.UUID;
import org.bukkit.Location;
import org.bukkit.World;
import org.bukkit.entity.Entity;
import org.bukkit.entity.EntityType;

/** Common Bukkit entity handle backed by a persistent UUID. */
public class FotonEntity implements Entity {
    private static final java.util.concurrent.ConcurrentHashMap<UUID, FotonPersistentDataContainer> DATA =
        new java.util.concurrent.ConcurrentHashMap<>();
    private final UUID id;
    public FotonEntity(UUID id) { this.id = id; }
    @Override public org.bukkit.persistence.PersistentDataContainer getPersistentDataContainer() {
        return DATA.computeIfAbsent(id, ignored -> new FotonPersistentDataContainer());
    }
    @Override public UUID getUniqueId() { return id; }
    @Override public Location getLocation() {
        double[] p = Native.entityPosition(id.toString());
        String world = Native.entityWorld(id.toString());
        return p == null || world == null ? null
            : new Location(new FotonWorld(world), p[0], p[1], p[2]);
    }
    @Override public World getWorld() {
        String world = Native.entityWorld(id.toString());
        return world == null ? null : new FotonWorld(world);
    }
    @Override public EntityType getType() {
        String type = Native.entityType(id.toString());
        if (type == null) return null;
        try { return EntityType.valueOf(type.toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException ignored) { return null; }
    }
    @Override public int getEntityId() { return Native.entityId(id.toString()); }
    @Override public boolean isDead() { return Native.entityWorld(id.toString()) == null; }
    @Override public String getCustomName() { return Native.entityCustomName(id.toString()); }
    @Override public void setCustomName(String name) { Native.setEntityCustomName(id.toString(), name); }
    @Override public io.papermc.paper.threadedregions.scheduler.EntityScheduler getScheduler() {
        return FotonRegionSchedulers.forEntity();
    }
    @Override public void sendMessage(String message) { Native.entitySendMessage(id.toString(), message); }
    @Override public boolean hasPermission(String permission) { return false; }
    @Override public String getName() { return getType() == null ? "" : getType().name(); }
}
