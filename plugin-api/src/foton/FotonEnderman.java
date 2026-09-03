package foton;

import org.bukkit.block.data.BlockData;

/** Live Bukkit view of a Steel Enderman. */
public final class FotonEnderman extends FotonLivingEntity implements org.bukkit.entity.Enderman {
    public FotonEnderman(java.util.UUID id) { super(id); }
    @Override public BlockData getCarriedBlock() {
        String value = Native.endermanCarriedBlock(getUniqueId().toString());
        return value == null ? null : org.bukkit.Bukkit.createBlockData(value);
    }
    @Override public void setCarriedBlock(BlockData block) {
        Native.setEndermanCarriedBlock(getUniqueId().toString(), block == null ? "" : block.getAsString());
    }
    @Override public org.bukkit.entity.EntityType getType() { return org.bukkit.entity.EntityType.ENDERMAN; }
}
