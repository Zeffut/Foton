package org.bukkit.block.sign;

import org.bukkit.block.Sign;

/** Live view of one side of a sign. */
public final class SignSide {
    private final Sign sign;
    private final Side side;
    public SignSide(Sign sign, Side side) { this.sign = sign; this.side = side; }
    public String getLine(int index) {
        if (sign instanceof foton.FotonSign live) return live.getSideLine(side, index);
        return index >= 0 && index < 4 ? "" : "";
    }
    /** Returns the four lines on this side in Bukkit order. */
    public String[] getLines() {
        String[] lines = new String[4];
        for (int index = 0; index < lines.length; index++) lines[index] = getLine(index);
        return lines;
    }
    public org.bukkit.DyeColor getColor() { return sign.getColor(); }
    public void setColor(org.bukkit.DyeColor color) { sign.setColor(color); }
    public boolean isGlowingText() { return sign instanceof foton.FotonSign live && live.isSideGlowing(side); }
    public void setGlowingText(boolean glowing) { if (sign instanceof foton.FotonSign live) live.setSideGlowing(side, glowing); }
    public net.kyori.adventure.text.Component line(int index) { return net.kyori.adventure.text.Component.text(getLine(index)); }
    public void setLine(int index, String line) {
        if (sign instanceof foton.FotonSign live) live.setSideLine(side, index, line);
    }
}
