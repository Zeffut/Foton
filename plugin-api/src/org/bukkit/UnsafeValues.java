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

    /** The entity's save data, as bytes a plugin can store and bring back. */
    default byte[] serializeEntity(org.bukkit.entity.Entity entity) {
        if (entity == null) throw new IllegalArgumentException("entity");
        byte[] data = foton.Native.serializeEntity(entity.getUniqueId().toString());
        if (data == null) throw new IllegalArgumentException("Couldn't serialize entity " + entity.getType());
        return data;
    }

    default org.bukkit.entity.Entity deserializeEntity(byte[] data, org.bukkit.World world) {
        return deserializeEntity(data, world, false);
    }

    /** The entity {@code data} describes, not yet in the world: place it with
     * {@code Entity.spawnAt}. It keeps its UUID only when asked to. */
    default org.bukkit.entity.Entity deserializeEntity(byte[] data, org.bukkit.World world, boolean preserveUUID) {
        if (data == null || world == null) throw new IllegalArgumentException("data and world are required");
        String id = foton.Native.deserializeEntity(data, world.getName(), preserveUUID);
        if (id == null) throw new IllegalArgumentException("Could not deserialize entity");
        return foton.FotonEntity.of(java.util.UUID.fromString(id));
    }
}
