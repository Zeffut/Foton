package org.bukkit.block;

/** A sign block state. */
public interface Sign extends TileState {
    default org.bukkit.DyeColor getColor() { return org.bukkit.DyeColor.BLACK; }
    boolean isWaxed();
    default void setWaxed(boolean waxed) { }
    default org.bukkit.block.sign.SignSide getSide(org.bukkit.block.sign.Side side) { return new org.bukkit.block.sign.SignSide(this, side); }
    String getLine(int index);
    default net.kyori.adventure.text.Component line(int index) { return net.kyori.adventure.text.Component.text(getLine(index)); }
    String[] getLines();
    void setLine(int index, String line);
    default void setColor(org.bukkit.DyeColor color) { }
    default void setGlowingText(boolean glowing) { getSide(org.bukkit.block.sign.Side.FRONT).setGlowingText(glowing); }
    default boolean isGlowingText() { return getSide(org.bukkit.block.sign.Side.FRONT).isGlowingText(); }
}
