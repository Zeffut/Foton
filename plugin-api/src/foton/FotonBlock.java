package foton;

import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.World;
import org.bukkit.block.Block;
import org.bukkit.block.BlockState;
import org.bukkit.block.data.BlockData;
import org.bukkit.block.data.SimpleBlockData;

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
    public BlockData getBlockData() {
        String text = world == null ? null : Native.blockState(world.getName(), x, y, z);
        return new SimpleBlockData(text);
    }

    @Override
    public BlockState getState() {
        return new FotonBlockState(this, getBlockData());
    }

    @Override
    public boolean isEmpty() {
        return getType().isAir();
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
