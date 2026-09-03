package org.bukkit;

/** The sixteen vanilla dye colors, in Bukkit's historical order. */
public enum DyeColor {
    WHITE(0), ORANGE(1), MAGENTA(2), LIGHT_BLUE(3), YELLOW(4), LIME(5), PINK(6),
    GRAY(7), LIGHT_GRAY(8), CYAN(9), PURPLE(10), BLUE(11), BROWN(12), GREEN(13),
    RED(14), BLACK(15);

    private final byte woolData;
    DyeColor(int woolData) { this.woolData = (byte) woolData; }
    public byte getWoolData() { return woolData; }
    /** Returns the vanilla firework RGB color for this dye. */
    public Color getFireworkColor() {
        return Color.fromRGB(foton.Native.dyeFireworkColor(ordinal()));
    }
    public Color getColor() {
        return Color.fromRGB(switch (this) {
            case WHITE -> 0xF9FFFE; case ORANGE -> 0xF9801D; case MAGENTA -> 0xC74EBD;
            case LIGHT_BLUE -> 0x3AB3DA; case YELLOW -> 0xFED83D; case LIME -> 0x80C71F;
            case PINK -> 0xF38BAA; case GRAY -> 0x474F52; case LIGHT_GRAY -> 0x9D9D97;
            case CYAN -> 0x169C9C; case PURPLE -> 0x8932B8; case BLUE -> 0x3C44AA;
            case BROWN -> 0x835432; case GREEN -> 0x5E7C16; case RED -> 0xB02E26;
            case BLACK -> 0x1D1D21;
        });
    }
    public static DyeColor getByWoolData(byte data) {
        int value = data & 0xff;
        return value < values().length ? values()[value] : null;
    }
    public static DyeColor getByColor(Color color) {
        if (color == null) return null;
        for (DyeColor dye : values()) if (dye.getColor().equals(color)) return dye;
        return null;
    }
}
