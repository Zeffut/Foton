import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.util.BoundingBox;
import org.bukkit.util.Vector;

/** Location and Vector, on the parts that are easy to get subtly wrong. */
final class Geometry {
    private Geometry() {}

    /** The box ray traces and entity sweeps are built on. */
    private static void boundingBox() {
        org.bukkit.util.BoundingBox box = new org.bukkit.util.BoundingBox(1, 0, 1, 0, 2, 0);
        Checks.same(box.getMinX(), 0.0, "corners are sorted");
        Checks.same(box.expandDirectional(-3, 0, 1).getMinX(), -3.0, "a sweep grows the face it moves toward");
        Checks.same(box.getMaxZ(), 2.0, "and only that face");
        Checks.same(box.clone().expand(1, 0, 0).getWidthX(), 6.0, "expand grows both faces of an axis");

        org.bukkit.util.BoundingBox unit = new org.bukkit.util.BoundingBox(0, 0, 0, 1, 1, 1);
        org.bukkit.util.RayTraceResult hit = unit.rayTrace(new Vector(-2, 0.5, 0.5), new Vector(1, 0, 0), 10);
        Checks.same(hit == null ? null : hit.getHitBlockFace(), org.bukkit.block.BlockFace.WEST,
            "a ray along +x enters through the west face");
        Checks.same(hit == null ? null : hit.getHitPosition().getX(), 0.0, "at the face itself");
        org.bukkit.util.RayTraceResult inside = unit.rayTrace(new Vector(0.5, 0.5, 0.5), new Vector(0, 1, 0), 10);
        Checks.same(inside == null ? null : inside.getHitBlockFace(), org.bukkit.block.BlockFace.UP,
            "a ray starting inside answers where it leaves");
        Checks.same(unit.rayTrace(new Vector(-2, 0.5, 0.5), new Vector(1, 0, 0), 1.5), null,
            "a box beyond the distance is missed");
    }

    static void check() {
        boundingBox();
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

        boundingBoxes();
        worldTypeDelegation();
    }

    private static void boundingBoxes() {
        BoundingBox expanded = new BoundingBox(0, 0, 0, 4, 6, 8);
        Checks.expect(expanded.expand(1, 2, 3) == expanded,
            "bounding box expansion mutates and returns the same box");
        Checks.same(expanded.getMinX(), -1.0, "uniform expansion moves the negative side");
        Checks.same(expanded.getMaxY(), 8.0, "uniform expansion moves the positive side");

        BoundingBox contracted = new BoundingBox(0, 0, 0, 4, 6, 8);
        contracted.expand(-3, -10, -1);
        Checks.same(contracted.getMinX(), 2.0,
            "over-contraction collapses an axis at its original center");
        Checks.same(contracted.getMaxX(), 2.0,
            "over-contraction never inverts an axis");
        Checks.same(contracted.getMinY(), 3.0,
            "each over-contracted axis uses its own center");
        Checks.same(contracted.getMinZ(), 1.0,
            "a bounded contraction keeps the remaining extent");
        Checks.same(contracted.getMaxZ(), 7.0,
            "a bounded contraction is symmetric");

        BoundingBox directional = new BoundingBox(0, 0, 0, 1, 1, 1);
        Checks.expect(directional.expandDirectional(-2, 3, 0.5) == directional,
            "directional expansion returns the same box");
        Checks.same(directional.getMinX(), -2.0,
            "a negative direction expands only the negative side");
        Checks.same(directional.getMaxX(), 1.0,
            "directional expansion leaves the opposite side fixed");
        Checks.same(directional.getMaxY(), 4.0,
            "a positive direction expands only the positive side");

        org.bukkit.util.VoxelShape shape = () -> java.util.List.of(
            new BoundingBox(0, 0, 0, 1, 1, 1),
            new BoundingBox(3, 0, 0, 4, 1, 1));
        Checks.expect(shape.overlaps(new BoundingBox(0.5, 0, 0, 1.5, 1, 1)),
            "a voxel shape overlaps when any constituent box overlaps");
        Checks.expect(!shape.overlaps(new BoundingBox(1, 0, 0, 2, 1, 1)),
            "boxes that only touch at a border do not overlap");
    }

    private static void worldTypeDelegation() {
        NamedWorld world = new NamedWorld("blocks");
        Checks.same(world.getType(7, 8, 9), Material.STONE,
            "World.getType delegates to the block lookup");
        Checks.same(java.util.List.of(world.lastBlockAt()[0], world.lastBlockAt()[1],
            world.lastBlockAt()[2]), java.util.List.of(7, 8, 9),
            "World.getType forwards every block coordinate");
    }

    /** A world that is only a name, so a location can have one without Foton. */
    private record NamedWorld(String name, int[] lastBlockAt) implements org.bukkit.World {
        private NamedWorld(String name) { this(name, new int[3]); }

        @Override public String getName() { return name; }

        @Override public java.util.UUID getUID() { return null; }

        @Override public org.bukkit.NamespacedKey getKey() { return null; }

        @Override public Location getSpawnLocation() { return null; }

        @Override public org.bukkit.block.Block getBlockAt(int x, int y, int z) {
            lastBlockAt[0] = x;
            lastBlockAt[1] = y;
            lastBlockAt[2] = z;
            return (org.bukkit.block.Block) java.lang.reflect.Proxy.newProxyInstance(
                Geometry.class.getClassLoader(),
                new Class<?>[] {org.bukkit.block.Block.class},
                (proxy, method, args) -> method.getName().equals("getType") ? Material.STONE : null);
        }

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

        @Override public java.util.List<org.bukkit.entity.Entity> getEntities() {
            return java.util.List.of();
        }

        @Override public org.bukkit.World.Environment getEnvironment() {
            return org.bukkit.World.Environment.CUSTOM;
        }
    }
}
