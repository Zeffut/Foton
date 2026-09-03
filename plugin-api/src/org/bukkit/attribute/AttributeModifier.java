package org.bukkit.attribute;

import java.util.UUID;
import org.bukkit.NamespacedKey;
import org.bukkit.inventory.EquipmentSlotGroup;

/** One additive/multiplicative attribute modifier. */
public final class AttributeModifier {
    public enum Operation { ADD_NUMBER, ADD_SCALAR, MULTIPLY_SCALAR_1 }
    private final UUID id;
    private final String name;
    private final double amount;
    private final Operation operation;
    private final NamespacedKey key;
    private final EquipmentSlotGroup slotGroup;
    private final org.bukkit.inventory.EquipmentSlot slot;
    public AttributeModifier(String name, double amount, Operation operation) {
        this(UUID.nameUUIDFromBytes((name == null ? "" : name).getBytes(java.nio.charset.StandardCharsets.UTF_8)), name, amount, operation, null, EquipmentSlotGroup.ANY, null);
    }
    public AttributeModifier(UUID id, String name, double amount, Operation operation) {
        this(id, name, amount, operation, null, EquipmentSlotGroup.ANY, null);
    }
    public AttributeModifier(NamespacedKey key, double amount, Operation operation, EquipmentSlotGroup slotGroup) {
        this(UUID.nameUUIDFromBytes(key.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8)), key.getKey(), amount, operation, key, slotGroup, null);
    }
    public AttributeModifier(UUID id, String name, double amount, Operation operation, org.bukkit.inventory.EquipmentSlot slot) {
        this(id, name, amount, operation, null, EquipmentSlotGroup.ANY, slot);
    }
    private AttributeModifier(UUID id, String name, double amount, Operation operation, NamespacedKey key, EquipmentSlotGroup slotGroup, org.bukkit.inventory.EquipmentSlot slot) {
        this.id = id == null ? UUID.randomUUID() : id; this.name = name == null ? "" : name;
        this.amount = amount; this.operation = operation == null ? Operation.ADD_NUMBER : operation;
        this.key = key;
        this.slotGroup = slotGroup == null ? EquipmentSlotGroup.ANY : slotGroup;
        this.slot = slot;
    }
    public UUID getUniqueId() { return id; }
    public String getName() { return name; }
    public double getAmount() { return amount; }
    public Operation getOperation() { return operation; }
    public NamespacedKey getKey() { return key; }
    public EquipmentSlotGroup getSlotGroup() { return slotGroup; }
    public org.bukkit.inventory.EquipmentSlot getSlot() { return slot; }
    public static AttributeModifier deserialize(java.util.Map<String, Object> values) {
        if (values == null) throw new IllegalArgumentException("values");
        Object name = values.get("name");
        Object amount = values.get("amount");
        Object operation = values.get("operation");
        if (!(amount instanceof Number) || operation == null) throw new IllegalArgumentException("Invalid attribute modifier");
        UUID uuid = values.get("uuid") == null ? null : UUID.fromString(String.valueOf(values.get("uuid")));
        Operation op = operation instanceof Operation value ? value : Operation.valueOf(String.valueOf(operation));
        return new AttributeModifier(uuid, name == null ? "" : String.valueOf(name), ((Number) amount).doubleValue(), op);
    }

    public java.util.Map<String, Object> serialize() {
        java.util.Map<String, Object> values = new java.util.LinkedHashMap<>();
        values.put("uuid", id.toString()); values.put("name", name); values.put("amount", amount); values.put("operation", operation.name());
        if (key != null) values.put("key", key.toString());
        if (slot != null) values.put("slot", slot.name());
        return values;
    }
}
