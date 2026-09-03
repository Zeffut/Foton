package org.bukkit.block.data;
import org.bukkit.block.BlockFace;
public interface Rotatable extends BlockData { BlockFace getRotation(); void setRotation(BlockFace rotation); }
