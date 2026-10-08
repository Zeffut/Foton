package foton;

import java.util.ArrayList;
import java.util.List;
import java.util.Objects;
import org.bukkit.DyeColor;
import org.bukkit.block.Banner;
import org.bukkit.block.banner.Pattern;

/** A banner as it was when the state was taken.
 *
 * <p>Its base color is the block's own, and its layers are the block entity's
 * {@code patterns}, read on first use. Both are edited on the snapshot and
 * written back by {@code update}, which is Bukkit's contract and how Paper's
 * {@code CraftBanner} behaves. */
public final class FotonBanner extends FotonTileState implements Banner {
    /** The layers, bottom first; null until something asks for them. */
    private List<Pattern> patterns;
    /** Whether the snapshot's layers differ from, or may have outlived, the block's. */
    private boolean patternsChanged;

    public FotonBanner(org.bukkit.block.Block block, org.bukkit.block.data.BlockData data) { super(block, data); }

    @Override public DyeColor getBaseColor() {
        String key = getType().getKeyName();
        for (DyeColor color : DyeColor.values())
            if (key.startsWith(color.name().toLowerCase(java.util.Locale.ROOT) + "_")) return color;
        return DyeColor.WHITE;
    }

    /** Recolors the snapshot's block, keeping its rotation or facing. */
    @Override public void setBaseColor(DyeColor color) {
        Objects.requireNonNull(color, "color");
        String current = getBlockData().getAsString();
        // "_banner" or "_wall_banner", whatever the current color.
        String suffix = getType().getKeyName().substring(getBaseColor().name().length());
        int properties = current.indexOf('[');
        String rest = properties < 0 ? "" : current.substring(properties);
        // Layers belong to the block entity, which a new block may replace.
        patterns();
        patternsChanged = true;
        setBlockData(org.bukkit.Bukkit.createBlockData(
            "minecraft:" + color.name().toLowerCase(java.util.Locale.ROOT) + suffix + rest));
    }

    private List<Pattern> patterns() {
        if (patterns == null) patterns = read();
        return patterns;
    }

    private List<Pattern> read() {
        String[] encoded = Native.bannerPatterns(getWorld().getName(), getX(), getY(), getZ());
        List<Pattern> result = new ArrayList<>();
        if (encoded != null) for (String value : encoded) {
            String[] parts = value == null ? new String[0] : value.split("\\|", 2);
            if (parts.length != 2) continue;
            try {
                result.add(new Pattern(DyeColor.values()[Integer.parseInt(parts[1])],
                    org.bukkit.Registry.BANNER_PATTERN.get(org.bukkit.NamespacedKey.fromString(parts[0]))));
            } catch (NumberFormatException | ArrayIndexOutOfBoundsException ignored) { }
        }
        return result;
    }

    @Override public List<Pattern> getPatterns() { return new ArrayList<>(patterns()); }

    @Override public void setPatterns(List<Pattern> layers) {
        Objects.requireNonNull(layers, "patterns");
        List<Pattern> copy = new ArrayList<>(layers.size());
        for (Pattern layer : layers) copy.add(checked(layer));
        patterns = copy;
        patternsChanged = true;
    }

    @Override public void addPattern(Pattern pattern) {
        patterns().add(checked(pattern));
        patternsChanged = true;
    }

    @Override public Pattern getPattern(int i) { return patterns().get(i); }

    @Override public Pattern removePattern(int i) {
        Pattern removed = patterns().remove(i);
        patternsChanged = true;
        return removed;
    }

    @Override public void setPattern(int i, Pattern pattern) {
        patterns().set(i, checked(pattern));
        patternsChanged = true;
    }

    @Override public int numberOfPatterns() { return patterns().size(); }

    private static Pattern checked(Pattern pattern) {
        Objects.requireNonNull(pattern, "pattern");
        Objects.requireNonNull(pattern.getPattern(), "pattern type");
        Objects.requireNonNull(pattern.getColor(), "pattern color");
        return pattern;
    }

    /** Writes the block, then the layers: the first can replace the block
     * entity the second belongs to. */
    @Override public boolean update(boolean force) {
        if (!super.update(force)) return false;
        if (!patternsChanged) return true;
        List<String> encoded = new ArrayList<>();
        for (Pattern layer : patterns)
            encoded.add(layer.getPattern().getKey() + "|" + layer.getColor().ordinal());
        boolean written = Native.setBannerPatterns(getWorld().getName(), getX(), getY(), getZ(), String.join(";", encoded));
        if (written) patternsChanged = false;
        return written;
    }
}
