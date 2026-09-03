package foton;

import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.World;
import org.bukkit.block.Block;
import org.bukkit.block.BlockState;
import org.bukkit.block.data.BlockData;
import org.bukkit.block.data.SimpleBlockData;
import org.bukkit.block.data.SimpleWaterloggedData;
import org.bukkit.block.data.type.SimpleTripwireData;

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
        if (key == null) return null;
        int colon = key.indexOf(':');
        String name = (colon < 0 ? key : key.substring(colon + 1)).toUpperCase(java.util.Locale.ROOT);
        try { return org.bukkit.block.Biome.valueOf(name); } catch (IllegalArgumentException ignored) { return null; }
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
        if (world != null && data != null) {
            Native.setBlock(world.getName(), x, y, z, data.getAsString());
        }
    }

    @Override
    public void setBlockData(BlockData data) {
        setBlockData(data, true);
    }

    @Override
    public void setType(Material type, boolean applyPhysics) {
        setType(type);
    }

    @Override
    public BlockData getBlockData() {
        String text = world == null ? null : Native.blockState(world.getName(), x, y, z);
        if (text != null && text.startsWith("minecraft:bell")) {
            return new org.bukkit.block.data.type.SimpleBellData(text);
        }
        if (text != null && text.startsWith("minecraft:cake")) {
            return new org.bukkit.block.data.type.SimpleCakeData(text);
        }
        if (text != null && text.startsWith("minecraft:piston_head")) {
            return new org.bukkit.block.data.type.SimplePistonHeadData(text);
        }
        if (text != null && text.contains("[rotation=")) {
            return new org.bukkit.block.data.SimpleRotatableData(text);
        }
        if (text != null && text.contains("[face=")) {
            return new org.bukkit.block.data.SimpleFaceAttachableData(text);
        }
        if (text != null && text.contains("[half=")) {
            if (text.contains("_door[")) return new org.bukkit.block.data.type.SimpleDoorData(text);
            return new org.bukkit.block.data.SimpleBisectedData(text);
        }
        if (text != null && text.contains("[level=")) {
            return new org.bukkit.block.data.SimpleLeveledData(text);
        }
        if (text != null && text.contains("[lit=")) {
            return new org.bukkit.block.data.SimpleLightableData(text);
        }
        if (text != null && text.startsWith("minecraft:tripwire")) {
            return new SimpleTripwireData(text);
        }
        if (text != null && text.contains("facing=")) {
            return new org.bukkit.block.data.SimpleDirectionalData(text);
        }
        if (text != null && text.contains("[waterlogged=")) {
            return new SimpleWaterloggedData(text);
        }
        return new SimpleBlockData(text);
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
        return new FotonBlockState(this, getBlockData());
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
