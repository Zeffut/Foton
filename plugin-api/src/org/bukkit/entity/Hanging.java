package org.bukkit.entity;

/** An entity attached to a block face, such as a painting or item frame. */
public interface Hanging extends Entity {
    default org.bukkit.block.BlockFace getAttachedFace() {
        String value = foton.Native.hangingFacing(getUniqueId().toString());
        if (value == null) return org.bukkit.block.BlockFace.NORTH;
        try { return org.bukkit.block.BlockFace.valueOf(value.toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException ignored) { return org.bukkit.block.BlockFace.NORTH; }
    }
    default org.bukkit.block.BlockFace getFacing() { return getAttachedFace(); }
}
