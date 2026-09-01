import org.bukkit.Location;
import org.bukkit.util.Vector;

/** Location and Vector, on the parts that are easy to get subtly wrong. */
final class Geometry {
    private Geometry() {}

    static void check() {
        Location at = new Location(null, 1.5, 64.0, -2.5, 90f, -10f);

        Checks.same(at.getX(), 1.5, "x");
        Checks.same(at.getYaw(), 90f, "yaw");

        // Floor, not truncation. -2.5 is in block -3; truncating would put it
        // in block -2 and quietly move everything on the negative side of the
        // world one block over.
        Checks.same(at.getBlockX(), 1, "a positive coordinate floors");
        Checks.same(at.getBlockZ(), -3, "a negative coordinate floors down, not toward zero");
        Checks.same(new Location(null, -0.5, 0, 0).getBlockX(), -1,
            "a coordinate just below zero is in block -1");

        // Arithmetic returns `this` and mutates, because Bukkit's does and
        // plugins chain off it.
        Location moved = at.add(1, 0, 0);
        Checks.expect(moved == at, "add returns the same object");
        Checks.same(at.getX(), 2.5, "add moved it");
        at.subtract(1, 0, 0);
        Checks.same(at.getX(), 1.5, "subtract moved it back");

        // A clone is separate, which is the whole reason plugins call it
        // before handing a location to anything.
        Location copy = at.clone();
        copy.add(100, 0, 0);
        Checks.same(at.getX(), 1.5, "a clone does not move the original");
        Checks.expect(at.equals(at.clone()), "a clone equals its original");

        Location other = new Location(null, 4.5, 64.0, -2.5);
        Checks.same(at.distance(other), 3.0, "distance in one axis");
        Checks.same(at.distanceSquared(other), 9.0, "distance squared avoids the root");

        // Two worlds have no distance between them, and Bukkit throws rather
        // than answering a number that would be wrong.
        boolean threw = false;
        try {
            at.distance(new Location(new NamedWorld("other"), 0, 0, 0));
        } catch (IllegalArgumentException expected) {
            threw = true;
        }
        Checks.expect(threw, "measuring between worlds should refuse");

        Vector vector = at.toVector();
        Checks.same(vector.getX(), 1.5, "toVector carries x");
        Checks.same(new Vector(3, 4, 0).length(), 5.0, "a three-four-five vector");
        Checks.same(new Vector(0, 0, 0).normalize().length(), 0.0,
            "normalizing nothing stays nothing rather than becoming NaN");
        Checks.same(new Vector(0, 5, 0).normalize().getY(), 1.0, "normalize scales to one");
        Location vectorLocation = new Vector(3, 4, 5).toLocation(null, 30, 15);
        Checks.same(vectorLocation.getX(), 3.0, "a vector carries x into a location");
        Checks.same(vectorLocation.getYaw(), 30f, "a vector carries yaw into a location");

        Location facing = new Location(null, 0, 0, 0);
        Vector south = facing.getDirection();
        Checks.expect(Math.abs(south.getX()) < 1.0e-12
            && Math.abs(south.getY()) < 1.0e-12
            && Math.abs(south.getZ() - 1.0) < 1.0e-12,
            "zero yaw and pitch face south");
        facing.setYaw(90);
        Vector west = facing.getDirection();
        Checks.expect(Math.abs(west.getX() + 1.0) < 1.0e-12
            && Math.abs(west.getZ()) < 1.0e-12,
            "positive ninety yaw faces west");
        facing.setPitch(90);
        Checks.expect(Math.abs(facing.getDirection().getY() + 1.0) < 1.0e-12,
            "positive ninety pitch faces down");

        Checks.same(org.bukkit.NamespacedKey.fromString("foton:overworld").getKey(), "overworld",
            "a namespaced key splits");
        Checks.same(org.bukkit.NamespacedKey.fromString("overworld").getNamespace(), "minecraft",
            "a bare key defaults to minecraft");
        Checks.same(org.bukkit.NamespacedKey.fromString(""), null,
            "an empty key is nobody's");
    }

    /** A world that is only a name, so a location can have one without Foton. */
    private record NamedWorld(String name) implements org.bukkit.World {
        @Override public String getName() { return name; }

        @Override public java.util.UUID getUID() { return null; }

        @Override public org.bukkit.NamespacedKey getKey() { return null; }

        @Override public Location getSpawnLocation() { return null; }

        @Override public org.bukkit.block.Block getBlockAt(int x, int y, int z) { return null; }

        @Override public org.bukkit.block.Block getBlockAt(Location location) { return null; }

        @Override public org.bukkit.Chunk getChunkAt(int x, int z) { return null; }

        @Override public org.bukkit.Chunk getChunkAt(Location location) { return null; }

        @Override public long getTime() { return 0; }

        @Override public long getFullTime() { return 0; }

        @Override public int getMinHeight() { return 0; }

        @Override public int getMaxHeight() { return 0; }

        @Override public java.util.List<org.bukkit.entity.Player> getPlayers() {
            return java.util.List.of();
        }

        @Override public org.bukkit.World.Environment getEnvironment() {
            return org.bukkit.World.Environment.CUSTOM;
        }
    }
}
