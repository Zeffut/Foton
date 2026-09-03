package org.bukkit.entity;

import java.util.UUID;
import org.bukkit.Location;
import org.bukkit.World;
import org.bukkit.command.CommandSender;

/** Anything in a world that has a position. */
public interface Entity extends CommandSender, org.bukkit.persistence.PersistentDataHolder, org.bukkit.metadata.Metadatable {
    default org.bukkit.event.entity.EntityDamageEvent getLastDamageCause() { return null; }
    default void setLastDamageCause(org.bukkit.event.entity.EntityDamageEvent event) { }

    default org.bukkit.util.BoundingBox getBoundingBox() { return null; }
    default float getYaw() { return 0.0f; }
    default float getPitch() { return 0.0f; }
    default boolean isOnGround() { return false; }
    default boolean isGlowing() { return false; }
    default void setGlowing(boolean glowing) { }
    default boolean isValid() { return !isDead(); }
    default boolean isInvulnerable() { return false; }
    default void setInvulnerable(boolean invulnerable) { }
    default void setMetadata(String key, org.bukkit.metadata.MetadataValue value) { foton.FotonMetadataBridge.set(this, key, value); }
    default java.util.List<org.bukkit.metadata.MetadataValue> getMetadata(String key) { return foton.FotonMetadataBridge.get(this, key); }
    default boolean hasMetadata(String key) { return !getMetadata(key).isEmpty(); }
    default void removeMetadata(String key, org.bukkit.plugin.Plugin plugin) { foton.FotonMetadataBridge.remove(this, key, plugin); }
    default boolean isPersistent() { return true; }
    default void setPersistent(boolean persistent) { }

    default org.bukkit.persistence.PersistentDataContainer getPersistentDataContainer() {
        return new foton.FotonPersistentDataContainer();
    }
    UUID getUniqueId();

    Location getLocation();
    default Location getOrigin() { return getLocation(); }
    default EntitySnapshot createSnapshot() { return new foton.FotonEntitySnapshot(getType(), getLocation()); }
    default org.bukkit.util.Vector getVelocity() { return new org.bukkit.util.Vector(); }
    default void setVelocity(org.bukkit.util.Vector velocity) { }
    default int getFireTicks() { return 0; }
    default void setFireTicks(int ticks) { }
    default int getPortalCooldown() { return 0; }
    default void setPortalCooldown(int ticks) { }
    default Location getEyeLocation() {
        Location location = getLocation();
        return location == null ? null : location.add(0.0, 1.62, 0.0);
    }
    default double getX() { return getLocation() == null ? 0.0 : getLocation().getX(); }
    default double getY() { return getLocation() == null ? 0.0 : getLocation().getY(); }
    default double getZ() { return getLocation() == null ? 0.0 : getLocation().getZ(); }

    World getWorld();
    default org.bukkit.Server getServer() { return org.bukkit.Bukkit.getServer(); }
    EntityType getType();
    default Entity getVehicle() { return null; }
    default boolean isInsideVehicle() { return getVehicle() != null; }
    default boolean leaveVehicle() { return false; }
    default java.util.List<Entity> getPassengers() { return java.util.Collections.emptyList(); }
    /** Returns true when this entity has no passengers. */
    default boolean isEmpty() { return getPassengers().isEmpty(); }
    default boolean addPassenger(Entity passenger) { return false; }
    default boolean removePassenger(Entity passenger) { return false; }
    default boolean setPassenger(Entity passenger) {
        if (passenger == null) return eject();
        eject();
        return addPassenger(passenger);
    }
    default boolean eject() { return false; }
    default SpawnCategory getSpawnCategory() { return SpawnCategory.MISC; }
    default org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason getEntitySpawnReason() { return org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason.DEFAULT; }
    default java.util.Collection<Entity> getNearbyEntities(double x, double y, double z) {
        Location location = getLocation();
        World world = getWorld();
        if (location == null || world == null) return java.util.Collections.emptyList();
        java.util.ArrayList<Entity> result = new java.util.ArrayList<>();
        for (Entity entity : world.getNearbyEntities(location, x, y, z))
            if (entity != this && !getUniqueId().equals(entity.getUniqueId())) result.add(entity);
        return java.util.Collections.unmodifiableList(result);
    }

    int getEntityId();

    default boolean teleport(Location location) { return false; }
    default boolean teleport(Location location, org.bukkit.event.player.PlayerTeleportEvent.TeleportCause cause) { return teleport(location); }
    default java.util.concurrent.CompletableFuture<Boolean> teleportAsync(Location location) {
        return java.util.concurrent.CompletableFuture.completedFuture(teleport(location));
    }
    default java.util.concurrent.CompletableFuture<Boolean> teleportAsync(Location location, org.bukkit.event.player.PlayerTeleportEvent.TeleportCause cause) {
        return java.util.concurrent.CompletableFuture.completedFuture(teleport(location, cause));
    }
    default java.util.concurrent.CompletableFuture<Boolean> teleportAsync(Location location, org.bukkit.event.player.PlayerTeleportEvent.TeleportCause cause, io.papermc.paper.entity.TeleportFlag... flags) {
        return teleportAsync(location, cause);
    }

    default void remove() { }

    boolean isDead();
    String getCustomName();
    default void setCustomNameVisible(boolean visible) { foton.Native.setEntityCustomNameVisible(getUniqueId().toString(), visible); }
    void setCustomName(String name);

    /** The scheduler for work that follows this entity. */
    io.papermc.paper.threadedregions.scheduler.EntityScheduler getScheduler();
}
