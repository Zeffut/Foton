package foton;

import java.util.List;
import org.bukkit.DyeColor;
import org.bukkit.block.Banner;
import org.bukkit.block.banner.Pattern;
import org.bukkit.block.banner.PatternType;

/** Banner state whose base color is derived from the current block material. */
public final class FotonBanner extends FotonTileState implements Banner {
    public FotonBanner(org.bukkit.block.Block block, org.bukkit.block.data.BlockData data) { super(block, data); }
    @Override public DyeColor getBaseColor() {
        String key = getType().getKeyName();
        for (DyeColor color : DyeColor.values())
            if (key.startsWith(color.name().toLowerCase(java.util.Locale.ROOT) + "_")) return color;
        return DyeColor.WHITE;
    }
    @Override public void setBaseColor(DyeColor color) {
        if (color == null) return;
        String key = getType().getKeyName();
        String suffix = key.endsWith("_wall_banner") ? "_wall_banner" : "_banner";
        Native.setBlock(getWorld().getName(), getX(), getY(), getZ(), "minecraft:" + color.name().toLowerCase(java.util.Locale.ROOT) + suffix);
    }
    @Override public List<Pattern> getPatterns() {
        String[] encoded = Native.bannerPatterns(getWorld().getName(), getX(), getY(), getZ());
        java.util.ArrayList<Pattern> result = new java.util.ArrayList<>();
        if (encoded != null) for (String value : encoded) {
            String[] parts = value == null ? new String[0] : value.split("\\|", 2);
            if (parts.length == 2) try {
                result.add(new Pattern(DyeColor.getByWoolData((byte) Integer.parseInt(parts[1])), PatternType.of(parts[0])));
            } catch (NumberFormatException ignored) { }
        }
        return result;
    }
    @Override public void addPattern(Pattern pattern) {
        if (pattern == null || pattern.getPattern() == null || pattern.getColor() == null) return;
        java.util.ArrayList<String> encoded = new java.util.ArrayList<>();
        for (Pattern current : getPatterns())
            encoded.add(current.getPattern().getIdentifier() + "|" + (current.getColor().getWoolData() & 255));
        encoded.add(pattern.getPattern().getIdentifier() + "|" + (pattern.getColor().getWoolData() & 255));
        Native.setBannerPatterns(getWorld().getName(), getX(), getY(), getZ(), String.join(";", encoded));
    }
    @Override public boolean update() { return super.update(); }
}
