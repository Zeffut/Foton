package foton;

import org.bukkit.block.Jukebox;
import org.bukkit.block.data.BlockData;

/** Live jukebox state. */
public final class FotonJukebox extends FotonTileState implements Jukebox {
    FotonJukebox(FotonBlock block, BlockData data) { super(block, data); }
    @Override public boolean isPlaying() { return Native.jukeboxIsPlaying(getWorld().getName(), getX(), getY(), getZ()); }
    @Override public org.bukkit.inventory.ItemStack getRecord() { return FotonInventory.decode(Native.jukeboxRecord(getWorld().getName(), getX(), getY(), getZ())); }
    @Override public void setRecord(org.bukkit.inventory.ItemStack record) { Native.jukeboxSetRecord(getWorld().getName(), getX(), getY(), getZ(), FotonInventory.encode(record)); }
}
