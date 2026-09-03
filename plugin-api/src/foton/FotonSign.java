package foton;

import org.bukkit.block.Sign;

/** Live-backed snapshot view of a sign's front text. */
public final class FotonSign extends FotonTileState implements Sign {
    public FotonSign(org.bukkit.block.Block block, org.bukkit.block.data.BlockData data) {
        super(block, data);
    }
    @Override public void setWaxed(boolean waxed) { Native.signSetWaxed(getWorld().getName(), getX(), getY(), getZ(), waxed); }
    @Override public boolean isWaxed() {
        return Native.signIsWaxed(getWorld().getName(), getX(), getY(), getZ());
    }
    public String getSideLine(org.bukkit.block.sign.Side side, int index) {
        if (index < 0 || index >= 4) throw new IndexOutOfBoundsException(index);
        String[] lines = Native.signSideLines(getWorld().getName(), getX(), getY(), getZ(), side != org.bukkit.block.sign.Side.BACK);
        return lines != null && index < lines.length && lines[index] != null ? lines[index] : "";
    }
    public void setSideLine(org.bukkit.block.sign.Side side, int index, String line) {
        if (index < 0 || index >= 4) throw new IndexOutOfBoundsException(index);
        Native.signSideSetLine(getWorld().getName(), getX(), getY(), getZ(), line == null ? "" : line, index, side != org.bukkit.block.sign.Side.BACK);
    }
    @Override public String getLine(int index) {
        if (index < 0 || index >= 4) throw new IndexOutOfBoundsException(index);
        String[] lines = Native.signLines(getWorld().getName(), getX(), getY(), getZ());
        return lines != null && index < lines.length && lines[index] != null ? lines[index] : "";
    }
    @Override public String[] getLines() {
        String[] lines = Native.signLines(getWorld().getName(), getX(), getY(), getZ());
        return lines == null || lines.length != 4 ? new String[] {"", "", "", ""} : lines.clone();
    }
    public void setSideGlowing(org.bukkit.block.sign.Side side, boolean glowing) { Native.signSetGlowing(getWorld().getName(), getX(), getY(), getZ(), side != org.bukkit.block.sign.Side.BACK, glowing); }
    public boolean isSideGlowing(org.bukkit.block.sign.Side side) { return Native.signGlowing(getWorld().getName(), getX(), getY(), getZ(), side != org.bukkit.block.sign.Side.BACK); }
    @Override public org.bukkit.DyeColor getColor() {
        int value = Native.signColor(getWorld().getName(), getX(), getY(), getZ(), true);
        org.bukkit.DyeColor[] colors = org.bukkit.DyeColor.values();
        return value >= 0 && value < colors.length ? colors[value] : org.bukkit.DyeColor.BLACK;
    }
    @Override public void setColor(org.bukkit.DyeColor color) {
        if (color != null) Native.signSetColor(getWorld().getName(), getX(), getY(), getZ(), color.ordinal());
    }
    @Override public void setLine(int index, String line) {
        if (index < 0 || index >= 4) throw new IndexOutOfBoundsException(index);
        Native.signSetLine(getWorld().getName(), getX(), getY(), getZ(), line == null ? "" : line, index);
    }
}
