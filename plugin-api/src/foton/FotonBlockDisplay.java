package foton;

import java.util.UUID;
import org.bukkit.block.data.BlockData;

/** Live Bukkit view of a Steel block display. */
public final class FotonBlockDisplay extends FotonEntity implements org.bukkit.entity.BlockDisplay {
    public FotonBlockDisplay(UUID id) { super(id); }
    @Override public void setBlock(BlockData data) {
        if (data != null) Native.setBlockDisplayBlock(getUniqueId().toString(), data.getAsString());
    }
    @Override public void setBrightness(org.bukkit.entity.Display.Brightness brightness) {
        if (brightness != null) Native.setBlockDisplayBrightness(getUniqueId().toString(), brightness.getBlockLight(), brightness.getSkyLight());
    }
    @Override public void setViewRange(float range) { Native.setBlockDisplayViewRange(getUniqueId().toString(), range); }
    @Override public void setShadowRadius(float radius) { Native.setBlockDisplayShadowRadius(getUniqueId().toString(), radius); }
    @Override public BlockData getBlock() { return new org.bukkit.block.data.SimpleBlockData("minecraft:air"); }
    @Override public void setTransformation(org.bukkit.util.Transformation value) {
        if (value == null) return;
        org.joml.Vector3f t = value.getTranslation(), s = value.getScale();
        org.joml.Quaternionf l = value.getLeftRotation(), r = value.getRightRotation();
        Native.setBlockDisplayTransformation(getUniqueId().toString(), t.x, t.y, t.z, s.x, s.y, s.z,
            l.x, l.y, l.z, l.w, r.x, r.y, r.z, r.w);
    }
}
