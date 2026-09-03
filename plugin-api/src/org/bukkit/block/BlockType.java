package org.bukkit.block;

import org.bukkit.Keyed;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;
import org.bukkit.block.data.BlockData;

/** Typed registry entry for a block type. */
public interface BlockType extends Keyed {
    Material getMaterial();
    default BlockData createBlockData() { return org.bukkit.Bukkit.createBlockData(getMaterial().getKey().toString()); }
    default BlockData createBlockData(String data) { return org.bukkit.Bukkit.createBlockData(data); }
}
