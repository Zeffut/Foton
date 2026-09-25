package org.bukkit.entity;

/** A human-controlled living entity. */
public interface HumanEntity extends LivingEntity, AnimalTamer, org.bukkit.inventory.InventoryHolder {
    /** Current hunger level, in the vanilla range 0..20. */
    int getFoodLevel();
    @Override org.bukkit.inventory.PlayerInventory getInventory();
    org.bukkit.inventory.Inventory getEnderChest();
    org.bukkit.inventory.InventoryView getOpenInventory();

    /** Opens an inventory for this human and returns the resulting view.
     *
     * <p>Returns null when the inventory could not be opened -- Bukkit's own
     * contract, and the reason plugins null-check the result before wiring
     * click handlers to it. */
    default org.bukkit.inventory.InventoryView openInventory(org.bukkit.inventory.Inventory inventory) {
        return null;
    }

    void closeInventory();

    /** Adds the recipe to this human's recipe book; whether it was new. */
    default boolean discoverRecipe(org.bukkit.NamespacedKey recipe) {
        return this.discoverRecipes(java.util.Arrays.asList(recipe)) != 0;
    }
    /** Adds the recipes to the recipe book, ignoring the ones already known; how many were new. */
    int discoverRecipes(java.util.Collection<org.bukkit.NamespacedKey> recipes);
    /** Takes the recipe out of the recipe book; whether it was known. */
    default boolean undiscoverRecipe(org.bukkit.NamespacedKey recipe) {
        return this.undiscoverRecipes(java.util.Arrays.asList(recipe)) != 0;
    }
    /** Takes the recipes out of the recipe book; how many were known. */
    int undiscoverRecipes(java.util.Collection<org.bukkit.NamespacedKey> recipes);
    boolean hasDiscoveredRecipe(org.bukkit.NamespacedKey recipe);
    /** An immutable set of every recipe in the recipe book. */
    java.util.Set<org.bukkit.NamespacedKey> getDiscoveredRecipes();

    /** An action bar message, formatting kept. */
    default void sendActionBar(net.kyori.adventure.text.Component message) {
        if (message != null) foton.Native.sendActionBarComponent(getUniqueId().toString(), foton.FotonComponents.toJson(message));
    }

    /** The stack held on the cursor in whatever screen is open. */
    default org.bukkit.inventory.ItemStack getItemOnCursor() {
        org.bukkit.inventory.ItemStack item = foton.FotonInventory.decode(foton.Native.playerCursor(getUniqueId().toString()));
        return item == null ? new org.bukkit.inventory.ItemStack(org.bukkit.Material.AIR) : item;
    }
    default void setItemOnCursor(org.bukkit.inventory.ItemStack item) {
        foton.Native.setPlayerCursor(getUniqueId().toString(), foton.FotonInventory.encode(item));
    }

    /** Ticks left on the cooldown group {@code key} -- an item id or a {@code use_cooldown} group. */
    default int getCooldown(net.kyori.adventure.key.Key key) {
        if (key == null) throw new IllegalArgumentException("key");
        return foton.Native.playerCooldown(getUniqueId().toString(), key.asString());
    }
    default void setCooldown(net.kyori.adventure.key.Key key, int ticks) {
        if (key == null) throw new IllegalArgumentException("key");
        if (ticks < 0) throw new IllegalArgumentException("Cannot have negative cooldown");
        foton.Native.setPlayerCooldown(getUniqueId().toString(), key.asString(), ticks);
    }
    default int getCooldown(org.bukkit.Material material) {
        if (material == null || !material.isItem()) throw new IllegalArgumentException("Material " + material + " is not an item");
        return getCooldown(material.getKey());
    }
    default void setCooldown(org.bukkit.Material material, int ticks) {
        if (material == null || !material.isItem()) throw new IllegalArgumentException("Material " + material + " is not an item");
        setCooldown(material.getKey(), ticks);
    }
    default boolean hasCooldown(org.bukkit.Material material) { return getCooldown(material) > 0; }
    /** A cooldown on the group this stack belongs to: its {@code use_cooldown} group, or its item. */
    default void setCooldown(org.bukkit.inventory.ItemStack item, int ticks) {
        if (item == null || item.getType().isAir()) throw new IllegalArgumentException("Cannot set cooldown on an empty item");
        if (ticks < 0) throw new IllegalArgumentException("Cannot have negative cooldown");
        foton.Native.setPlayerItemCooldown(getUniqueId().toString(), foton.FotonInventory.encode(item), ticks);
    }

    /** Opens a trading screen on {@code merchant}. Without {@code force} a
     * merchant already trading refuses and this answers null; with it, the
     * current trader's screen is closed first. */
    default org.bukkit.inventory.InventoryView openMerchant(org.bukkit.inventory.Merchant merchant, boolean force) {
        if (merchant == null) throw new IllegalArgumentException("merchant cannot be null");
        String handle;
        if (merchant instanceof foton.FotonMerchant custom) handle = custom.handle();
        else if (merchant instanceof Entity entity) handle = entity.getUniqueId().toString();
        else throw new IllegalArgumentException("Can't open merchant " + merchant);
        if (!foton.Native.openMerchant(getUniqueId().toString(), handle, force)) return null;
        return getOpenInventory();
    }
}
