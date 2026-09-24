package foton;

import java.util.UUID;
import org.bukkit.Color;
import org.bukkit.entity.Display;
import org.bukkit.util.Transformation;

/** What block, item and text displays share: vanilla's {@code Display} data. */
public abstract class FotonDisplay extends FotonEntity implements Display {
    // Where each field sits in Native.displayState.
    private static final int INTERPOLATION_DURATION = 0, INTERPOLATION_DELAY = 1, TELEPORT_DURATION = 2,
        BILLBOARD = 3, BRIGHTNESS = 4, VIEW_RANGE = 5, SHADOW_RADIUS = 6, SHADOW_STRENGTH = 7,
        WIDTH = 8, HEIGHT = 9, GLOW_COLOR = 10, TRANSFORMATION = 11, LENGTH = 25;

    protected FotonDisplay(UUID id) { super(id); }

    private double[] state() {
        double[] state = Native.displayState(getUniqueId().toString());
        return state == null || state.length < LENGTH ? null : state;
    }

    private double field(int index, double otherwise) {
        double[] state = state();
        return state == null ? otherwise : state[index];
    }

    private String id() { return getUniqueId().toString(); }

    @Override public Transformation getTransformation() {
        double[] s = state();
        if (s == null) {
            return new Transformation(new org.joml.Vector3f(), new org.joml.Quaternionf(),
                new org.joml.Vector3f(1, 1, 1), new org.joml.Quaternionf());
        }
        int t = TRANSFORMATION;
        return new Transformation(
            new org.joml.Vector3f((float) s[t], (float) s[t + 1], (float) s[t + 2]),
            new org.joml.Quaternionf((float) s[t + 6], (float) s[t + 7], (float) s[t + 8], (float) s[t + 9]),
            new org.joml.Vector3f((float) s[t + 3], (float) s[t + 4], (float) s[t + 5]),
            new org.joml.Quaternionf((float) s[t + 10], (float) s[t + 11], (float) s[t + 12], (float) s[t + 13]));
    }

    @Override public void setTransformation(Transformation transformation) {
        if (transformation == null) throw new IllegalArgumentException("Transformation cannot be null");
        org.joml.Vector3f t = transformation.getTranslation(), s = transformation.getScale();
        org.joml.Quaternionf l = transformation.getLeftRotation(), r = transformation.getRightRotation();
        Native.setDisplayTransformation(id(), t.x, t.y, t.z, s.x, s.y, s.z, l.x, l.y, l.z, l.w, r.x, r.y, r.z, r.w);
    }

    @Override public int getInterpolationDuration() { return (int) field(INTERPOLATION_DURATION, 0); }
    @Override public void setInterpolationDuration(int duration) { Native.setDisplayInterpolationDuration(id(), duration); }

    @Override public int getTeleportDuration() { return (int) field(TELEPORT_DURATION, 0); }
    @Override public void setTeleportDuration(int duration) {
        if (duration < 0 || duration > 59) throw new IllegalArgumentException("duration must be a value between 0 and 59 ticks");
        Native.setDisplayTeleportDuration(id(), duration);
    }

    @Override public float getViewRange() { return (float) field(VIEW_RANGE, 1.0); }
    @Override public void setViewRange(float range) { Native.setDisplayFloat(id(), VIEW_RANGE, range); }
    @Override public float getShadowRadius() { return (float) field(SHADOW_RADIUS, 0.0); }
    @Override public void setShadowRadius(float radius) { Native.setDisplayFloat(id(), SHADOW_RADIUS, radius); }
    @Override public float getShadowStrength() { return (float) field(SHADOW_STRENGTH, 1.0); }
    @Override public void setShadowStrength(float strength) { Native.setDisplayFloat(id(), SHADOW_STRENGTH, strength); }
    @Override public float getDisplayWidth() { return (float) field(WIDTH, 0.0); }
    @Override public void setDisplayWidth(float width) { Native.setDisplayFloat(id(), WIDTH, width); }
    @Override public float getDisplayHeight() { return (float) field(HEIGHT, 0.0); }
    @Override public void setDisplayHeight(float height) { Native.setDisplayFloat(id(), HEIGHT, height); }

    @Override public int getInterpolationDelay() { return (int) field(INTERPOLATION_DELAY, 0); }
    @Override public void setInterpolationDelay(int ticks) { Native.setDisplayInterpolationDelay(id(), ticks); }

    @Override public Billboard getBillboard() {
        int billboard = (int) field(BILLBOARD, 0);
        Billboard[] values = Billboard.values();
        return billboard >= 0 && billboard < values.length ? values[billboard] : Billboard.FIXED;
    }
    @Override public void setBillboard(Billboard billboard) {
        if (billboard == null) throw new IllegalArgumentException("Billboard cannot be null");
        Native.setDisplayBillboard(id(), billboard.ordinal());
    }

    @Override public Color getGlowColorOverride() {
        int color = (int) field(GLOW_COLOR, -1);
        return color == -1 ? null : Color.fromARGB(color);
    }
    @Override public void setGlowColorOverride(Color color) {
        Native.setDisplayGlowColor(id(), color == null ? -1 : color.asARGB());
    }

    @Override public Brightness getBrightness() {
        int packed = (int) field(BRIGHTNESS, -1);
        return packed == -1 ? null : new Brightness((packed >> 4) & 0xF, (packed >> 20) & 0xF);
    }
    @Override public void setBrightness(Brightness brightness) {
        if (brightness == null) Native.setDisplayBrightness(id(), -1, -1);
        else Native.setDisplayBrightness(id(), brightness.getBlockLight(), brightness.getSkyLight());
    }
}
