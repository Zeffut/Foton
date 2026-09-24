package foton;

import org.bukkit.inventory.EntityEquipment;
import org.bukkit.inventory.ItemStack;

/** Live equipment of any living entity, player or mob, slot by slot. */
final class FotonEntityEquipment implements EntityEquipment {
    private static final java.util.concurrent.ConcurrentHashMap<String, float[]> CHANCES = new java.util.concurrent.ConcurrentHashMap<>();
    private final String owner;
    private final float[] dropChances;
    FotonEntityEquipment(String owner) {
        this.owner = owner;
        dropChances = CHANCES.computeIfAbsent(owner, ignored -> new float[] {0.085f, 0.085f, 0.085f, 0.085f, 0.085f});
    }
    @Override public org.bukkit.entity.Entity getHolder() { return FotonEntity.of(Native.parse(owner)); }

    @Override public ItemStack getItem(org.bukkit.inventory.EquipmentSlot slot) {
        if (slot == null) throw new IllegalArgumentException("slot");
        return FotonInventory.decode(Native.entityEquipmentItem(owner, slot.ordinal()));
    }
    @Override public void setItem(org.bukkit.inventory.EquipmentSlot slot, ItemStack item) {
        if (slot == null) throw new IllegalArgumentException("slot");
        Native.setEntityEquipmentItem(owner, slot.ordinal(), FotonInventory.encode(item));
    }
    @Override public ItemStack[] getArmorContents() {
        return new ItemStack[] { getBoots(), getLeggings(), getChestplate(), getHelmet() };
    }
    @Override public ItemStack getHelmet() { return getItem(org.bukkit.inventory.EquipmentSlot.HEAD); }
    @Override public void setHelmet(ItemStack item) { setItem(org.bukkit.inventory.EquipmentSlot.HEAD, item); }
    @Override public ItemStack getChestplate() { return getItem(org.bukkit.inventory.EquipmentSlot.CHEST); }
    @Override public void setChestplate(ItemStack item) { setItem(org.bukkit.inventory.EquipmentSlot.CHEST, item); }
    @Override public ItemStack getLeggings() { return getItem(org.bukkit.inventory.EquipmentSlot.LEGS); }
    @Override public void setLeggings(ItemStack item) { setItem(org.bukkit.inventory.EquipmentSlot.LEGS, item); }
    @Override public ItemStack getBoots() { return getItem(org.bukkit.inventory.EquipmentSlot.FEET); }
    @Override public void setBoots(ItemStack item) { setItem(org.bukkit.inventory.EquipmentSlot.FEET, item); }
    @Override public void setArmorContents(ItemStack[] items) {
        org.bukkit.inventory.EquipmentSlot[] slots = {
            org.bukkit.inventory.EquipmentSlot.FEET, org.bukkit.inventory.EquipmentSlot.LEGS,
            org.bukkit.inventory.EquipmentSlot.CHEST, org.bukkit.inventory.EquipmentSlot.HEAD };
        for (int i = 0; i < slots.length; i++) setItem(slots[i], items != null && i < items.length ? items[i] : null);
    }
    @Override public ItemStack getItemInMainHand() { return getItem(org.bukkit.inventory.EquipmentSlot.HAND); }
    @Override public void setItemInMainHand(ItemStack item) { setItem(org.bukkit.inventory.EquipmentSlot.HAND, item); }
    @Override public ItemStack getItemInOffHand() { return getItem(org.bukkit.inventory.EquipmentSlot.OFF_HAND); }
    @Override public void setItemInOffHand(ItemStack item) { setItem(org.bukkit.inventory.EquipmentSlot.OFF_HAND, item); }
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
        for (org.bukkit.inventory.EquipmentSlot slot : org.bukkit.inventory.EquipmentSlot.values()) setItem(slot, null);
    }
}
