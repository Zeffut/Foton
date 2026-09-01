package org.bukkit.entity;

import java.util.UUID;
import org.bukkit.Location;
import org.bukkit.World;
import org.bukkit.command.CommandSender;

/** Anything in a world that has a position. */
public interface Entity extends CommandSender {
    default org.bukkit.persistence.PersistentDataContainer getPersistentDataContainer() {
        return new foton.FotonPersistentDataContainer();
    }
    UUID getUniqueId();

    Location getLocation();

    World getWorld();
    EntityType getType();
    default Entity getVehicle() { return null; }
    default SpawnCategory getSpawnCategory() { return SpawnCategory.MISC; }

    int getEntityId();

    default boolean teleport(Location location) { return false; }
    default boolean teleport(Location location, org.bukkit.event.player.PlayerTeleportEvent.TeleportCause cause) { return teleport(location); }
    default java.util.concurrent.CompletableFuture<Boolean> teleportAsync(Location location) {
        return java.util.concurrent.CompletableFuture.completedFuture(teleport(location));
    }
    default java.util.concurrent.CompletableFuture<Boolean> teleportAsync(Location location, org.bukkit.event.player.PlayerTeleportEvent.TeleportCause cause) {
        return java.util.concurrent.CompletableFuture.completedFuture(teleport(location, cause));
    }

    default void remove() { }

    boolean isDead();
    String getCustomName();
    void setCustomName(String name);

    /** The scheduler for work that follows this entity. */
    io.papermc.paper.threadedregions.scheduler.EntityScheduler getScheduler();
}
