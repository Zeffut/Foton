package org.bukkit.entity;

import java.util.UUID;
import org.bukkit.Location;
import org.bukkit.World;
import org.bukkit.command.CommandSender;

/** Anything in a world that has a position. */
public interface Entity extends CommandSender, org.bukkit.Nameable, org.bukkit.persistence.PersistentDataHolder, org.bukkit.metadata.Metadatable {
    /** Distance fallen since the entity was last on the ground.
     *
     * <p>Declared here rather than only on {@code LivingEntity} because
     * plugins reference it through {@code Entity} -- an arrow's or a boat's
     * fall distance is as real as a zombie's. */
    default float getFallDistance() { return 0.0f; }

    default void setFallDistance(float distance) { }

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

    /** Copies this entity's position and rotation into {@code location}, and
     * returns it -- the allocation-free form hot loops use. */
    default Location getLocation(Location location) {
        Location current = getLocation();
        if (location == null || current == null) return location;
        location.setWorld(current.getWorld());
        location.setX(current.getX());
        location.setY(current.getY());
        location.setZ(current.getZ());
        location.setYaw(current.getYaw());
        location.setPitch(current.getPitch());
        return location;
    }

    /** The height of the entity's current bounding box: its pose and scale included. */
    default double getHeight() {
        double[] box = foton.Native.entityBoundingBox(getUniqueId().toString());
        return box == null || box.length < 6 ? 0.0 : box[4] - box[1];
    }

    /** The width of the entity's current bounding box. */
    default double getWidth() {
        double[] box = foton.Native.entityBoundingBox(getUniqueId().toString());
        return box == null || box.length < 6 ? 0.0 : box[3] - box[0];
    }

    default boolean hasGravity() { return foton.Native.entityGravity(getUniqueId().toString()); }
    default void setGravity(boolean gravity) { foton.Native.setEntityGravity(getUniqueId().toString(), gravity); }
    default boolean isSilent() { return foton.Native.entitySilent(getUniqueId().toString()); }
    default void setSilent(boolean silent) { foton.Native.setEntitySilent(getUniqueId().toString(), silent); }

    /** Turns the entity, head included; the angles are wrapped and clamped first. */
    default void setRotation(float yaw, float pitch) {
        if (!Float.isFinite(yaw) || !Float.isFinite(pitch)) throw new IllegalArgumentException("yaw and pitch must be finite");
        foton.Native.setEntityRotation(getUniqueId().toString(), Location.normalizeYaw(yaw), Location.normalizePitch(pitch));
    }

    default Pose getPose() {
        int pose = foton.Native.entityPose(getUniqueId().toString());
        Pose[] poses = Pose.values();
        return pose >= 0 && pose < poses.length ? poses[pose] : Pose.STANDING;
    }

    /** The entity's scoreboard tags: live, so adding to the set tags the entity. */
    default java.util.Set<String> getScoreboardTags() { return new foton.FotonScoreboardTags(getUniqueId().toString()); }
    default boolean addScoreboardTag(String tag) { return tag != null && foton.Native.addEntityTag(getUniqueId().toString(), tag); }
    default boolean removeScoreboardTag(String tag) { return tag != null && foton.Native.removeEntityTag(getUniqueId().toString(), tag); }

    /** The custom name as a component. Foton keeps the name's text here; its
     * formatting reaches players but is not read back. */
    @Override default net.kyori.adventure.text.Component customName() {
        String name = getCustomName();
        return name == null ? null : net.kyori.adventure.text.Component.text(name);
    }

    /** Sets the custom name, colour and formatting included. */
    @Override default void customName(net.kyori.adventure.text.Component customName) {
        foton.Native.setEntityCustomNameComponent(getUniqueId().toString(), foton.FotonComponents.toJson(customName));
    }
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
    default boolean teleport(Location location, io.papermc.paper.entity.TeleportFlag... flags) {
        return teleport(location, org.bukkit.event.player.PlayerTeleportEvent.TeleportCause.PLUGIN, flags);
    }
    /** Paper's flagged teleport. The base form ignores the flags; Foton's
     * entities apply them (see {@code FotonEntity}). */
    default boolean teleport(Location location, org.bukkit.event.player.PlayerTeleportEvent.TeleportCause cause, io.papermc.paper.entity.TeleportFlag... flags) {
        return teleport(location, cause);
    }
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

    /** Spawns an entity that is not in a world yet -- one from
     * {@code UnsafeValues.deserializeEntity} -- at {@code location}, firing the
     * spawn event with {@code reason}. False if it was already spawned, the
     * event was cancelled, or the chunk is not loaded. */
    default boolean spawnAt(Location location, org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason reason) {
        if (location == null || location.getWorld() == null) throw new IllegalArgumentException("location");
        return foton.Native.spawnPendingEntity(getUniqueId().toString(), location.getWorld().getName(),
            location.getX(), location.getY(), location.getZ(), location.getYaw(), location.getPitch(),
            reason == null ? "DEFAULT" : reason.name());
    }
    default boolean spawnAt(Location location) {
        return spawnAt(location, org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason.DEFAULT);
    }

    boolean isDead();
    @Override String getCustomName();
    default void setCustomNameVisible(boolean visible) { foton.Native.setEntityCustomNameVisible(getUniqueId().toString(), visible); }
    @Override void setCustomName(String name);

    /** The scheduler for work that follows this entity. */
    io.papermc.paper.threadedregions.scheduler.EntityScheduler getScheduler();

    /** Whether the entity is touching lava. */
    default boolean isInLava() { return (foton.Native.entitySurroundings(getUniqueId().toString()) & 2) != 0; }
    /** Whether the entity is touching water; a bubble column is water. */
    default boolean isInWaterOrBubbleColumn() { return (foton.Native.entitySurroundings(getUniqueId().toString()) & 4) != 0; }
    /** Whether rain is falling on the entity's position. */
    default boolean isInRain() { return (foton.Native.entitySurroundings(getUniqueId().toString()) & 16) != 0; }
    default boolean isInWaterOrRain() { return (foton.Native.entitySurroundings(getUniqueId().toString()) & (4 | 16)) != 0; }
    default boolean isInWaterOrRainOrBubbleColumn() { return isInWaterOrRain(); }
}
