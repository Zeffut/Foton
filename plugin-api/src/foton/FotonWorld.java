package foton;

import java.util.UUID;
import org.bukkit.Chunk;
import org.bukkit.Location;
import org.bukkit.NamespacedKey;
import org.bukkit.World;
import org.bukkit.block.Block;

/** A world, as a plugin holds one.
 *
 * A name and a route back into Foton, for the same reason a player is a UUID:
 * a handle that outlives what it names should stop answering rather than keep
 * a dead thing alive.
 */
public final class FotonWorld implements World {
    private final String name;

    public FotonWorld(String name) {
        this.name = name;
    }

    @Override
    public String getName() {
        return name;
    }

    /** A stable id for this world, derived from its name.
     *
     * Foton identifies a world by its key rather than by a UUID, so there is
     * no stored one to hand back. Deriving it from the name gives a plugin
     * what it actually wants -- an identity that is the same across restarts
     * and different between worlds -- without inventing a number and calling
     * it saved.
     */
    @Override
    public UUID getUID() {
        return UUID.nameUUIDFromBytes(
            ("foton:world:" + name).getBytes(java.nio.charset.StandardCharsets.UTF_8));
    }

    @Override
    public NamespacedKey getKey() {
        return NamespacedKey.fromString(name);
    }

    @Override
    public Location getSpawnLocation() {
        double[] at = Native.worldSpawn(name);
        return at == null ? null : new Location(this, at[0], at[1], at[2], (float) at[3],
            (float) at[4]);
    }

    @Override
    public Block getBlockAt(int x, int y, int z) {
        return new FotonBlock(this, x, y, z);
    }

    @Override
    public Block getBlockAt(Location location) {
        return getBlockAt(location.getBlockX(), location.getBlockY(), location.getBlockZ());
    }

    @Override
    public Chunk getChunkAt(int x, int z) {
        return new FotonChunk(this, x, z);
    }

    @Override
    public Chunk getChunkAt(Location location) {
        // A chunk is sixteen blocks wide, and the shift is an arithmetic one so
        // that negative coordinates land in the chunk below rather than
        // rounding toward zero into their neighbour.
        return getChunkAt(location.getBlockX() >> 4, location.getBlockZ() >> 4);
    }

    @Override
    public long getTime() {
        long full = getFullTime();
        return full < 0 ? 0 : full % 24000L;
    }

    @Override
    public long getFullTime() {
        return Native.worldTime(name);
    }

    @Override public int getMinHeight() { return Native.worldMinHeight(name); }
    @Override public int getMaxHeight() { return Native.worldMaxHeight(name); }

    @Override
    public java.util.List<org.bukkit.entity.Player> getPlayers() {
        String[] ids = Native.worldPlayerIds(name);
        java.util.ArrayList<org.bukkit.entity.Player> players = new java.util.ArrayList<>(ids.length);
        for (String id : ids) {
            players.add(new FotonPlayer(UUID.fromString(id)));
        }
        return java.util.Collections.unmodifiableList(players);
    }

    @Override
    public Environment getEnvironment() {
        return switch (name) {
            case "minecraft:overworld" -> Environment.NORMAL;
            case "minecraft:the_nether" -> Environment.NETHER;
            case "minecraft:the_end" -> Environment.THE_END;
            default -> Environment.CUSTOM;
        };
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof FotonWorld world && name.equals(world.name);
    }

    @Override
    public int hashCode() {
        return name.hashCode();
    }

    @Override
    public String toString() {
        return "FotonWorld{" + name + "}";
    }
}
