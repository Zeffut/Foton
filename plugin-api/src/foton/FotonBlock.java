package foton;

import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.World;
import org.bukkit.block.Block;
import org.bukkit.block.BlockState;
import org.bukkit.block.data.BlockData;

/** A block, as a plugin holds one: a world and three coordinates.
 *
 * Nothing is cached. A plugin that kept one of these across a few ticks and
 * read it again should see what is there now, which is what Bukkit's own Block
 * does and the reason BlockState exists separately as a snapshot.
 */
public final class FotonBlock implements Block {
    private final World world;
    private final int x;
    private final int y;
    private final int z;

    public FotonBlock(World world, int x, int y, int z) {
        this.world = world;
        this.x = x;
        this.y = y;
        this.z = z;
    }

    @Override
    public int getX() {
        return x;
    }

    @Override
    public int getY() {
        return y;
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
    public Location getLocation() {
        return new Location(world, x, y, z);
    }

    @Override
    public org.bukkit.block.Biome getBiome() {
        String key = world == null ? null : Native.biomeKey(world.getName(), x, y, z);
        return key == null ? null : org.bukkit.Registry.BIOME.get(org.bukkit.NamespacedKey.fromString(key));
    }

    @Override
    public Material getType() {
        return getBlockData().getMaterial();
    }

    @Override
    public void setType(Material type) {
        if (world != null && type != null) {
            Native.setBlock(world.getName(), x, y, z, "minecraft:" + type.getKeyName());
        }
    }

    @Override
    public void setBlockData(BlockData data, boolean applyPhysics) {
        if (world == null || data == null) return;
        if (applyPhysics) Native.setBlock(world.getName(), x, y, z, data.getAsString());
        else Native.setBlockWithoutPhysics(world.getName(), x, y, z, data.getAsString());
    }

    @Override
    public void setBlockData(BlockData data) {
        setBlockData(data, true);
    }

    @Override
    public void setType(Material type, boolean applyPhysics) {
        if (type == null) return;
        if (applyPhysics) setType(type);
        else if (world != null) Native.setBlockWithoutPhysics(world.getName(), x, y, z, "minecraft:" + type.getKeyName());
    }

    @Override
    public BlockData getBlockData() {
        return dataOf(world == null ? null : Native.blockState(world.getName(), x, y, z));
    }

    /** The typed Bukkit view of a block state written `minecraft:name[props]`. */
    static BlockData dataOf(String text) {
        return org.bukkit.block.data.PropertyBlockData.of(text);
    }

    @Override
    public BlockState getState() {
        String key = getBlockData().getMaterial().getKeyName();
        if (key.endsWith("_sign") || key.endsWith("_wall_sign")) {
            return new FotonSign(this, getBlockData());
        }
        if (key.endsWith("_banner") || key.endsWith("_wall_banner")) {
            return new FotonBanner(this, getBlockData());
        }
        if (getType() == Material.CHISELED_BOOKSHELF) {
            return new FotonChiseledBookshelf(this, getBlockData());
        }
        if (getType() == Material.JUKEBOX) {
            return new FotonJukebox(this, getBlockData());
        }
        if (getType() == Material.HOPPER) {
            return new FotonHopper(this, getBlockData());
        }
        if (getType() == Material.FURNACE || getType() == Material.BLAST_FURNACE
                || getType() == Material.SMOKER) {
            return FotonFurnace.of(this, getBlockData());
        }
        if (getType() == Material.BREWING_STAND) {
            return FotonBrewingStand.placedSnapshot(this, getBlockData());
        }
        if (getType() == Material.CRAFTER) {
            return new FotonCrafter(this, getBlockData());
        }
        if (getType() == Material.DISPENSER) {
            return new FotonDispenser(this, getBlockData());
        }
        if (getType() == Material.LECTERN) {
            return new FotonLectern(this, getBlockData());
        }
        if (getType() == Material.SPAWNER) {
            return new FotonCreatureSpawner(this, getBlockData());
        }
        BlockData data = getBlockData();
        byte[] tileData = world == null ? null : Native.blockEntityPersistentData(world.getName(), x, y, z);
        return tileData == null ? new FotonBlockState(this, data) : new FotonTileState(this, data, tileData);
    }

    @Override
    public boolean isEmpty() {
        return getType().isAir();
    }

    @Override public byte getLightFromBlocks() {
        return world == null ? 0 : Native.blockLight(world.getName(), x, y, z);
    }

    @Override public boolean isBlockIndirectlyPowered() { return world != null && Native.blockIndirectlyPowered(world.getName(), x, y, z); }

    @Override public byte getLightFromSky() {
        return world == null ? 0 : Native.skyLight(world.getName(), x, y, z);
    }

    @Override public boolean breakNaturally() {
        return world != null && Native.breakBlock(world.getName(), x, y, z);
    }

    @Override
    public Block getRelative(org.bukkit.block.BlockFace face) {
        return getRelative(face, 1);
    }

    @Override
    public Block getRelative(org.bukkit.block.BlockFace face, int distance) {
        return face == null
            ? this
            : getRelative(face.getModX() * distance, face.getModY() * distance,
                face.getModZ() * distance);
    }

    @Override
    public Block getRelative(int dx, int dy, int dz) {
        return new FotonBlock(world, x + dx, y + dy, z + dz);
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof FotonBlock block
            && x == block.x && y == block.y && z == block.z
            && java.util.Objects.equals(world, block.world);
    }

    @Override
    public int hashCode() {
        return java.util.Objects.hash(world, x, y, z);
    }

    @Override
    public String toString() {
        return "FotonBlock{" + (world == null ? "?" : world.getName())
            + " " + x + ", " + y + ", " + z + "}";
    }
}
