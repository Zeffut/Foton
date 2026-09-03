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

    @Override public org.bukkit.entity.Entity[] getEntities() {
        java.util.ArrayList<org.bukkit.entity.Entity> result = new java.util.ArrayList<>();
        String worldName = world.getName();
        String[] ids = Native.worldEntityIds(worldName);
        if (ids == null) return new org.bukkit.entity.Entity[0];
        for (String id : ids) {
            double[] position = Native.entityPosition(id);
            if (position == null) continue;
            int entityChunkX = ((int) Math.floor(position[0])) >> 4;
            int entityChunkZ = ((int) Math.floor(position[2])) >> 4;
            if (entityChunkX != x || entityChunkZ != z) continue;
            try { result.add(FotonWorld.wrapEntity(java.util.UUID.fromString(id), id)); }
            catch (IllegalArgumentException ignored) { }
        }
        return result.toArray(new org.bukkit.entity.Entity[0]);
    }

    @Override public org.bukkit.block.BlockState[] getTileEntities() {
        java.util.ArrayList<org.bukkit.block.BlockState> result = new java.util.ArrayList<>();
        String[] encoded = Native.chunkBlockEntities(world.getName(), x, z);
        if (encoded == null) return new org.bukkit.block.BlockState[0];
        for (String value : encoded) {
            String[] fields = value.split("\\|", -1);
            if (fields.length != 4) continue;
            try {
                result.add(new FotonBlock(world, Integer.parseInt(fields[0]), Integer.parseInt(fields[1]), Integer.parseInt(fields[2])).getState());
            } catch (NumberFormatException ignored) { }
        }
        return result.toArray(new org.bukkit.block.BlockState[0]);
    }

    @Override
    public org.bukkit.block.BlockState[] getTileEntities(boolean useSnapshot) {
        return getTileEntities();
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
