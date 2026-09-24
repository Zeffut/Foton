package foton;

import java.util.UUID;
import org.bukkit.block.data.BlockData;

/** Live Bukkit view of a block display. */
public final class FotonBlockDisplay extends FotonDisplay implements org.bukkit.entity.BlockDisplay {
    public FotonBlockDisplay(UUID id) { super(id); }

    @Override public BlockData getBlock() {
        String state = Native.blockDisplayBlock(getUniqueId().toString());
        return org.bukkit.Bukkit.createBlockData(state == null ? "minecraft:air" : state);
    }

    @Override public void setBlock(BlockData data) {
        if (data == null) throw new IllegalArgumentException("Block cannot be null");
        Native.setBlockDisplayBlock(getUniqueId().toString(), data.getAsString());
    }
}
