package org.bukkit;

/** Limited server implementation hooks exposed by Bukkit. */
public interface UnsafeValues {
    int getDataVersion();
    /** Applies Vanilla item SNBT modifications to a copy of the supplied stack. */
    default org.bukkit.inventory.ItemStack modifyItemStack(org.bukkit.inventory.ItemStack stack, String arguments) { return stack == null ? null : stack.clone(); }
    /** Protocol number for the Minecraft version implemented by Foton. */
    default int getProtocolVersion() { return 776; }
    default org.bukkit.block.data.BlockData fromLegacy(Material material, byte data) { return material == null ? null : material.createBlockData(); }
    default Material fromLegacy(org.bukkit.material.MaterialData material) { return material == null ? null : material.getItemType(); }
    default NamespacedKey getBiomeKey(RegionAccessor region, int x, int y, int z) { if (region == null) return null; org.bukkit.block.Block block=region.getBlockAt(x,y,z); org.bukkit.block.Biome biome=block==null?null:block.getBiome(); return biome==null?null:biome.getKey(); }
}
