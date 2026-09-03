package foton;

import java.util.UUID;
import org.bukkit.Location;
import org.bukkit.World;
import org.bukkit.entity.Entity;
import org.bukkit.entity.EntityType;

/** Common Bukkit entity handle backed by a persistent UUID. */
public class FotonEntity implements Entity, org.bukkit.projectiles.ProjectileSource {
    @Override public org.bukkit.event.entity.EntityDamageEvent getLastDamageCause() {
        return EventBridge.lastDamageCause(getUniqueId());
    }
    @Override public void setLastDamageCause(org.bukkit.event.entity.EntityDamageEvent event) {
        EventBridge.setLastDamageCause(getUniqueId(), event);
    }
    @Override public org.bukkit.util.BoundingBox getBoundingBox() {
        double[] b = Native.entityBoundingBox(getUniqueId().toString());
        return b == null || b.length < 6 ? null : new org.bukkit.util.BoundingBox(b[0], b[1], b[2], b[3], b[4], b[5]);
    }
    @Override public float getYaw() { double[] p = Native.entityPosition(getUniqueId().toString()); return p == null || p.length < 4 ? 0.0f : (float) p[3]; }
    @Override public float getPitch() { double[] p = Native.entityPosition(getUniqueId().toString()); return p == null || p.length < 5 ? 0.0f : (float) p[4]; }
    @Override public boolean isOnGround() { return Native.entityOnGround(id.toString()); }
    @Override public boolean isValid() { return Native.entityWorld(getUniqueId().toString()) != null; }
    @Override public boolean isInvulnerable() { return Native.entityInvulnerable(id.toString()); }
    @Override public void setInvulnerable(boolean invulnerable) { Native.setEntityInvulnerable(id.toString(), invulnerable); }
    @Override public boolean isGlowing() { return Native.entityGlowing(id.toString()); }
    @Override public void setGlowing(boolean glowing) { Native.setEntityGlowing(id.toString(), glowing); }
    private static final java.util.concurrent.ConcurrentHashMap<UUID, FotonPersistentDataContainer> DATA =
        new java.util.concurrent.ConcurrentHashMap<>();
    private final UUID id;
    public FotonEntity(UUID id) { this.id = id; }
    public static FotonEntity handle(UUID id) {
        if (id == null) return null;
        org.bukkit.entity.Entity wrapped = FotonWorld.wrapEntity(id, Native.entityType(id.toString()));
        return wrapped instanceof FotonEntity entity ? entity : new FotonEntity(id);
    }
    @Override public boolean isPersistent() { return Native.entityPersistent(getUniqueId().toString()); }
    @Override public void setPersistent(boolean persistent) { Native.setEntityPersistent(getUniqueId().toString(), persistent); }

    @Override public org.bukkit.persistence.PersistentDataContainer getPersistentDataContainer() {
        return DATA.computeIfAbsent(id, ignored -> new FotonPersistentDataContainer());
    }
    @Override public boolean equals(Object other) {
        return other instanceof org.bukkit.entity.Entity entity && id.equals(entity.getUniqueId());
    }

    @Override public int hashCode() { return id.hashCode(); }

    @Override public UUID getUniqueId() { return id; }
    @Override public Location getLocation() {
        double[] p = Native.entityPosition(id.toString());
        String world = Native.entityWorld(id.toString());
        return p == null || world == null ? null
            : new Location(new FotonWorld(world), p[0], p[1], p[2]);
    }
    @Override public Location getOrigin() {
        double[] p = Native.entityOrigin(id.toString());
        String world = Native.entityWorld(id.toString());
        return p == null || world == null ? null : new Location(new FotonWorld(world), p[0], p[1], p[2]);
    }
    @Override public org.bukkit.util.Vector getVelocity() {
        double[] velocity = Native.entityVelocity(id.toString());
        return velocity == null || velocity.length < 3 ? new org.bukkit.util.Vector() : new org.bukkit.util.Vector(velocity[0], velocity[1], velocity[2]);
    }
    @Override public void setVelocity(org.bukkit.util.Vector velocity) {
        if (velocity != null) Native.setEntityVelocity(id.toString(), velocity.getX(), velocity.getY(), velocity.getZ());
    }
    @Override public int getFireTicks() { return Native.entityFireTicks(id.toString()); }
    @Override public void setFireTicks(int ticks) { Native.setEntityFireTicks(id.toString(), ticks); }
    @Override public int getPortalCooldown() { return Native.entityPortalCooldown(id.toString()); }
    @Override public void setPortalCooldown(int ticks) { Native.setEntityPortalCooldown(id.toString(), ticks); }
    @Override public Location getEyeLocation() {
        Location location = getLocation();
        if (location != null) location.setY(location.getY() + Native.entityEyeHeight(id.toString()));
        return location;
    }
    @Override public World getWorld() {
        String world = Native.entityWorld(id.toString());
        return world == null ? null : new FotonWorld(world);
    }
    @Override public boolean eject() { return Native.entityEject(id.toString()); }

    @Override public boolean leaveVehicle() { return Native.entityLeaveVehicle(id.toString()); }

    @Override public Entity getVehicle() {
        String vehicle = Native.entityVehicle(id.toString());
        try { return vehicle == null ? null : FotonEntity.handle(UUID.fromString(vehicle)); }
        catch (IllegalArgumentException error) { return null; }
    }
    @Override public java.util.List<Entity> getPassengers() {
        String encoded = Native.entityPassengers(id.toString());
        if (encoded == null || encoded.isEmpty()) return java.util.List.of();
        java.util.ArrayList<Entity> result = new java.util.ArrayList<>();
        for (String value : encoded.split(",")) try { result.add(FotonEntity.handle(UUID.fromString(value))); }
        catch (IllegalArgumentException ignored) { }
        return java.util.Collections.unmodifiableList(result);
    }
    @Override public boolean removePassenger(Entity passenger) { return passenger != null && Native.entityRemovePassenger(id.toString(), passenger.getUniqueId().toString()); }

    @Override public boolean addPassenger(Entity passenger) {
        return passenger != null && Native.entityAddPassenger(id.toString(), passenger.getUniqueId().toString());
    }
    @Override public boolean setPassenger(Entity passenger) {
        if (passenger == null) return eject();
        eject();
        return addPassenger(passenger);
    }

    @Override public EntityType getType() {
        String type = Native.entityType(id.toString());
        if (type == null) return null;
        try { return EntityType.valueOf(type.toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException ignored) { return null; }
    }
    @Override public org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason getEntitySpawnReason() {
        String reason = Native.entitySpawnReason(id.toString());
        if (reason == null) return org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason.DEFAULT;
        try { return org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason.valueOf(reason); }
        catch (IllegalArgumentException ignored) { return org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason.CUSTOM; }
    }
    @Override public org.bukkit.entity.SpawnCategory getSpawnCategory() {
        String category = Native.entitySpawnCategory(id.toString());
        if (category == null) return org.bukkit.entity.SpawnCategory.MISC;
        try { return org.bukkit.entity.SpawnCategory.valueOf(category.toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException ignored) { return org.bukkit.entity.SpawnCategory.MISC; }
    }
    @Override public int getEntityId() { return Native.entityId(id.toString()); }
    @Override public boolean teleport(Location location) {
        if (location == null || location.getWorld() == null) return false;
        return Native.teleportEntity(id.toString(), location.getWorld().getName(), location.getX(), location.getY(), location.getZ(), location.getYaw(), location.getPitch());
    }

    @Override public void remove() { Native.removeEntity(id.toString()); }

    @Override public java.util.concurrent.CompletableFuture<Boolean> teleportAsync(Location location, org.bukkit.event.player.PlayerTeleportEvent.TeleportCause cause) {
        return java.util.concurrent.CompletableFuture.completedFuture(teleport(location, cause));
    }

    @Override public boolean isDead() { return Native.entityWorld(id.toString()) == null; }
    @Override public String getCustomName() { return Native.entityCustomName(id.toString()); }
    @Override public void setCustomNameVisible(boolean visible) { Native.setEntityCustomNameVisible(id.toString(), visible); }
    @Override public void setCustomName(String name) { Native.setEntityCustomName(id.toString(), name); }
    @Override public io.papermc.paper.threadedregions.scheduler.EntityScheduler getScheduler() {
        return FotonRegionSchedulers.forEntity();
    }
    @Override public void sendMessage(String message) { Native.entitySendMessage(id.toString(), message); }
    @Override public boolean hasPermission(String permission) { return false; }
    @Override public String getName() { return getType() == null ? "" : getType().name(); }
}
