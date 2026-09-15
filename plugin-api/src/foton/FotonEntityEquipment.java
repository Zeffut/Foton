package foton;

import org.bukkit.inventory.EntityEquipment;
import org.bukkit.inventory.EquipmentSlot;
import org.bukkit.inventory.ItemStack;

/** Equipment view backed by a living entity's live slots. */
final class FotonEntityEquipment implements EntityEquipment {
    private static final java.util.concurrent.ConcurrentHashMap<String, float[]> CHANCES = new java.util.concurrent.ConcurrentHashMap<>();
    private final String owner;
    private final float[] dropChances;
    FotonEntityEquipment(String owner) {
        this.owner = owner;
        dropChances = CHANCES.computeIfAbsent(owner, ignored -> new float[] {0.085f, 0.085f, 0.085f, 0.085f, 0.085f});
    }
    @Override public org.bukkit.entity.Entity getHolder() {
        try { return new FotonEntity(java.util.UUID.fromString(owner)); }
        catch (IllegalArgumentException ignored) { return null; }
    }
    @Override public ItemStack[] getArmorContents() {
        return new ItemStack[] {
            getItem(EquipmentSlot.FEET), getItem(EquipmentSlot.LEGS),
            getItem(EquipmentSlot.CHEST), getItem(EquipmentSlot.HEAD)
        };
    }
    @Override public ItemStack getHelmet() { return getItem(EquipmentSlot.HEAD); }
    @Override public void setHelmet(ItemStack item) { setItem(EquipmentSlot.HEAD, item); }
    @Override public ItemStack getChestplate() { return getItem(EquipmentSlot.CHEST); }
    @Override public void setChestplate(ItemStack item) { setItem(EquipmentSlot.CHEST, item); }
    @Override public ItemStack getBoots() { return getItem(EquipmentSlot.FEET); }
    @Override public void setBoots(ItemStack item) { setItem(EquipmentSlot.FEET, item); }
    @Override public ItemStack getLeggings() { return getItem(EquipmentSlot.LEGS); }
    @Override public void setLeggings(ItemStack item) { setItem(EquipmentSlot.LEGS, item); }
    @Override public void setArmorContents(ItemStack[] items) {
        setItem(EquipmentSlot.FEET, items != null && items.length > 0 ? items[0] : null);
        setItem(EquipmentSlot.LEGS, items != null && items.length > 1 ? items[1] : null);
        setItem(EquipmentSlot.CHEST, items != null && items.length > 2 ? items[2] : null);
        setItem(EquipmentSlot.HEAD, items != null && items.length > 3 ? items[3] : null);
    }
    @Override public ItemStack getItemInMainHand() { return getItem(EquipmentSlot.HAND); }
    @Override public void setItemInMainHand(ItemStack item) { setItem(EquipmentSlot.HAND, item); }
    @Override public ItemStack getItemInOffHand() { return getItem(EquipmentSlot.OFF_HAND); }
    @Override public void setItemInOffHand(ItemStack item) { setItem(EquipmentSlot.OFF_HAND, item); }
    @Override public ItemStack getItem(EquipmentSlot slot) {
        if (slot == null) throw new IllegalArgumentException("slot cannot be null");
        ItemStack item = FotonInventory.decode(Native.entityEquipmentSlot(owner, slotIndex(slot)));
        return item == null ? new ItemStack(org.bukkit.Material.AIR) : item;
    }
    @Override public void setItem(EquipmentSlot slot, ItemStack item) {
        if (slot == null) throw new IllegalArgumentException("slot cannot be null");
        Native.setEntityEquipmentSlot(owner, slotIndex(slot), FotonInventory.encode(item));
    }
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
        Native.clearEntityEquipment(owner);
    }
    private static int slotIndex(EquipmentSlot slot) {
        return switch (slot) {
            case HAND -> 0;
            case OFF_HAND -> 1;
            case FEET -> 2;
            case LEGS -> 3;
            case CHEST -> 4;
            case HEAD -> 5;
            case BODY -> 6;
            case SADDLE -> 7;
        };
    }
}
