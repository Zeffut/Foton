package com.destroystokyo.paper.entity;

import java.util.List;
import org.bukkit.Location;
import org.bukkit.entity.Entity;
import org.bukkit.entity.LivingEntity;
import org.bukkit.entity.Mob;

/** A mob's path finding: where it is walking, and asking it to walk somewhere. */
public interface Pathfinder {
    Mob getEntity();

    /** Stops following the current path, if any. */
    void stopPathfinding();

    /** Whether the mob is following a path it has not finished. */
    boolean hasPath();

    /** The path being followed, or null. */
    PathResult getCurrentPath();

    default boolean moveTo(Location loc) { return moveTo(loc, 1); }

    /** Finds a path to {@code loc} and starts along it at {@code speed} times
     * the mob's movement speed; false when there is none. */
    boolean moveTo(Location loc, double speed);

    default boolean moveTo(LivingEntity target) { return moveTo(target, 1); }
    default boolean moveTo(LivingEntity target, double speed) { return moveTo((Entity) target, speed); }
    default boolean moveTo(Entity target) { return moveTo(target, 1); }
    /** Paths to where the target stands now, as vanilla's {@code moveTo(Entity)} does. */
    default boolean moveTo(Entity target, double speed) {
        Location location = target == null ? null : target.getLocation();
        return location != null && moveTo(location, speed);
    }

    /** A path: its points, in order, and how far along it the mob is. */
    interface PathResult {
        List<Location> getPoints();
        int getNextPointIndex();
        Location getNextPoint();
        Location getFinalPoint();
        boolean canReachFinalPoint();
    }
}
