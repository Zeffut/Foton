package foton;

import java.util.UUID;
import org.bukkit.DyeColor;
import org.bukkit.block.BlockFace;

/** Live Bukkit view of a shulker. */
public final class FotonShulker extends FotonLivingEntity implements org.bukkit.entity.Shulker {
    public FotonShulker(UUID id) { super(id); }

    private double[] state() {
        double[] state = Native.shulkerState(getUniqueId().toString());
        return state == null || state.length < 3 ? null : state;
    }

    /** Vanilla stores the peek as a percentage. */
    @Override public float getPeek() { double[] s = state(); return s == null ? 0.0f : (float) s[0] / 100.0f; }
    @Override public void setPeek(float value) {
        if (value < 0.0f || value > 1.0f) throw new IllegalArgumentException("value needs to be in between or equal to 0 and 1");
        Native.setShulkerPeek(getUniqueId().toString(), (int) (value * 100.0f));
    }

    @Override public BlockFace getAttachedFace() {
        String face = Native.shulkerAttachedFace(getUniqueId().toString());
        try { return face == null ? BlockFace.DOWN : BlockFace.valueOf(face); }
        catch (IllegalArgumentException ignored) { return BlockFace.DOWN; }
    }
    @Override public void setAttachedFace(BlockFace face) {
        if (face == null) throw new IllegalArgumentException("face cannot be null");
        switch (face) {
            case DOWN, UP, NORTH, SOUTH, WEST, EAST -> { }
            default -> throw new IllegalArgumentException("face must be one of the six cartesian faces");
        }
        Native.setShulkerAttachedFace(getUniqueId().toString(), face.name());
    }

    /** The shell's dye colour, or null for the undyed purple one. */
    @Override public DyeColor getColor() {
        double[] s = state();
        int color = s == null ? -1 : (int) s[2];
        DyeColor[] colors = DyeColor.values();
        return color >= 0 && color < colors.length ? colors[color] : null;
    }
    @Override public void setColor(DyeColor color) { Native.setShulkerColor(getUniqueId().toString(), color == null ? -1 : color.ordinal()); }
}
