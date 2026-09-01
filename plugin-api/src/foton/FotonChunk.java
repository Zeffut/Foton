package foton;

import org.bukkit.Chunk;
import org.bukkit.World;

/** A chunk, as a plugin holds one: its coordinates and its world. */
public final class FotonChunk implements Chunk {
    private static final java.util.concurrent.ConcurrentHashMap<String, FotonPersistentDataContainer> DATA =
        new java.util.concurrent.ConcurrentHashMap<>();
    private final World world;
    private final int x;
    private final int z;

    public FotonChunk(World world, int x, int z) {
        this.world = world;
        this.x = x;
        this.z = z;
    }

    @Override
    public int getX() {
        return x;
    }

    @Override
    public int getZ() {
        return z;
    }

    @Override
    public World getWorld() {
        return world;
    }

    @Override
    public org.bukkit.persistence.PersistentDataContainer getPersistentDataContainer() {
        String key = world.getUID() + ":" + x + ":" + z;
        return DATA.computeIfAbsent(key, ignored -> new FotonPersistentDataContainer());
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof FotonChunk chunk
            && x == chunk.x && z == chunk.z && world.equals(chunk.world);
    }

    @Override
    public int hashCode() {
        return java.util.Objects.hash(world, x, z);
    }

    @Override
    public String toString() {
        return "FotonChunk{" + world.getName() + " " + x + ", " + z + "}";
    }
}
