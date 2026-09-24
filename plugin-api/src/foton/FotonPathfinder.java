package foton;

import com.destroystokyo.paper.entity.Pathfinder;
import java.util.List;
import org.bukkit.Location;
import org.bukkit.entity.Mob;

/** A mob's own navigation, driven from Java: the path finding its goals use. */
public final class FotonPathfinder implements Pathfinder {
    private final Mob mob;

    public FotonPathfinder(Mob mob) { this.mob = mob; }

    private String id() { return mob.getUniqueId().toString(); }

    @Override public Mob getEntity() { return mob; }
    @Override public void stopPathfinding() { Native.mobStopPathfinding(id()); }
    @Override public boolean hasPath() { return Native.mobHasPath(id()); }

    @Override public boolean moveTo(Location loc, double speed) {
        if (loc == null) throw new IllegalArgumentException("location");
        org.bukkit.World world = mob.getWorld();
        if (world == null || loc.getWorld() != null && !loc.getWorld().getName().equals(world.getName())) return false;
        return Native.mobMoveTo(id(), loc.getX(), loc.getY(), loc.getZ(), speed);
    }

    @Override public PathResult getCurrentPath() {
        double[] path = Native.mobCurrentPath(id());
        org.bukkit.World world = mob.getWorld();
        if (path == null || path.length < 2 || world == null) return null;
        java.util.ArrayList<Location> points = new java.util.ArrayList<>();
        for (int i = 2; i + 2 < path.length; i += 3) points.add(new Location(world, path[i], path[i + 1], path[i + 2]));
        int next = (int) path[0];
        boolean reaches = path[1] != 0;
        List<Location> fixed = java.util.Collections.unmodifiableList(points);
        return new PathResult() {
            @Override public List<Location> getPoints() { return fixed; }
            @Override public int getNextPointIndex() { return next; }
            @Override public Location getNextPoint() { return next >= 0 && next < fixed.size() ? fixed.get(next) : null; }
            @Override public Location getFinalPoint() { return fixed.isEmpty() ? null : fixed.get(fixed.size() - 1); }
            @Override public boolean canReachFinalPoint() { return reaches; }
        };
    }
}
