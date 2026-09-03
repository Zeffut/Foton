package foton;

import org.bukkit.inventory.EntityEquipment;
import org.bukkit.inventory.ItemStack;

/** Equipment view backed by a player's live inventory slots. */
final class FotonEntityEquipment implements EntityEquipment {
    private static final java.util.concurrent.ConcurrentHashMap<String, float[]> CHANCES = new java.util.concurrent.ConcurrentHashMap<>();
    private final FotonInventory inventory;
    private final String owner;
    private final float[] dropChances;
    FotonEntityEquipment(String owner) {
        this.owner = owner;
        inventory = new FotonInventory(owner);
        dropChances = CHANCES.computeIfAbsent(owner, ignored -> new float[] {0.085f, 0.085f, 0.085f, 0.085f, 0.085f});
    }
    @Override public org.bukkit.entity.Entity getHolder() {
        try { return new FotonEntity(java.util.UUID.fromString(owner)); }
        catch (IllegalArgumentException ignored) { return null; }
    }
    @Override public ItemStack[] getArmorContents() { return inventory.getArmorContents(); }
    @Override public ItemStack getHelmet() { return inventory.getItem(39); }
    @Override public void setHelmet(ItemStack item) { inventory.setItem(39, item); }
    @Override public ItemStack getChestplate() { return inventory.getItem(38); }
    @Override public void setChestplate(ItemStack item) { inventory.setItem(38, item); }
    @Override public ItemStack getBoots() { return inventory.getItem(36); }
    @Override public void setBoots(ItemStack item) { inventory.setItem(36, item); }
    @Override public ItemStack getLeggings() { return inventory.getItem(38); }
    @Override public void setLeggings(ItemStack item) { inventory.setItem(38, item); }
    @Override public void setArmorContents(ItemStack[] items) { for (int i = 0; i < 4; i++) inventory.setItem(36 + i, items != null && i < items.length ? items[i] : null); }
    @Override public ItemStack getItemInMainHand() { return inventory.getItemInMainHand(); }
    @Override public void setItemInMainHand(ItemStack item) { inventory.setItemInMainHand(item); }
    @Override public ItemStack getItemInOffHand() { return inventory.getItemInOffHand(); }
    @Override public void setItemInOffHand(ItemStack item) { inventory.setItemInOffHand(item); }
    @Override public float getItemInHandDropChance() { return nativeChance(0, 4); }
    @Override public void setItemInHandDropChance(float chance) { setNativeChance(0, 4, chance); }
    @Override public float getItemInMainHandDropChance() { return getItemInHandDropChance(); }
    @Override public void setItemInMainHandDropChance(float chance) { setItemInHandDropChance(chance); }
    @Override public float getHelmetDropChance() { return nativeChance(5, 0); }
    @Override public void setHelmetDropChance(float chance) { setNativeChance(5, 0, chance); }
    @Override public float getChestplateDropChance() { return nativeChance(4, 1); }
    @Override public void setChestplateDropChance(float chance) { setNativeChance(4, 1, chance); }
    @Override public float getLeggingsDropChance() { return nativeChance(3, 2); }
    @Override public void setLeggingsDropChance(float chance) { setNativeChance(3, 2, chance); }
    @Override public float getBootsDropChance() { return nativeChance(2, 3); }
    @Override public void setBootsDropChance(float chance) { setNativeChance(2, 3, chance); }
    private float nativeChance(int slot, int index) { float value = Native.entityDropChance(owner, slot); return value < 0.0f ? dropChances[index] : value; }
    private void setNativeChance(int slot, int index, float chance) { dropChances[index] = validateChance(chance); Native.setEntityDropChance(owner, slot, dropChances[index]); }
    private static float validateChance(float chance) {
        if (!Float.isFinite(chance) || chance < 0.0f || chance > 1.0f) throw new IllegalArgumentException("Drop chance must be between 0 and 1");
        return chance;
    }
    @Override public void clear() {
        for (int slot = 36; slot <= 40; slot++) inventory.setItem(slot, null);
    }
}
