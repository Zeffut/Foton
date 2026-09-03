package foton;

import org.bukkit.UnsafeValues;

/** Values sourced from the targeted vanilla Minecraft sources. */
public final class FotonUnsafeValues implements UnsafeValues {
    @Override public int getDataVersion() { return 4903; }

    @Override public org.bukkit.inventory.ItemStack modifyItemStack(org.bukkit.inventory.ItemStack stack, String arguments) {
        if (stack == null) return null;
        if (arguments == null || arguments.isBlank()) return stack.clone();
        String existing = stack.getOpaqueNbt();
        String merged = Native.mergeItemSnbt(existing == null ? "{}" : existing, arguments);
        if (merged == null) throw new IllegalArgumentException("Invalid item SNBT: " + arguments);
        org.bukkit.inventory.ItemStack result = stack.clone();
        result.setOpaqueNbt(merged);
        return result;
    }
}
