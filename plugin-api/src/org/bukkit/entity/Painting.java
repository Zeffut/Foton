package org.bukkit.entity;

import org.bukkit.Art;

public interface Painting extends Hanging {
    default org.bukkit.block.BlockFace getFacing() { return getAttachedFace(); }
    default boolean setFacingDirection(org.bukkit.block.BlockFace face, boolean force) { return face != null && foton.Native.setHangingFacing(getUniqueId().toString(), face.name(), force); }
    Art getArt();
    boolean setArt(Art art);
    boolean setArt(Art art, boolean force);
}
