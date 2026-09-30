package org.bukkit.attribute;

import java.nio.charset.StandardCharsets;
import java.util.UUID;
import java.util.regex.Pattern;
import org.bukkit.NamespacedKey;
import org.bukkit.inventory.EquipmentSlot;
import org.bukkit.inventory.EquipmentSlotGroup;

/** One modifier on an attribute, identified by its key as in vanilla since 1.21. */
public class AttributeModifier implements org.bukkit.configuration.serialization.ConfigurationSerializable, org.bukkit.Keyed {
    public enum Operation { ADD_NUMBER, ADD_SCALAR, MULTIPLY_SCALAR_1 }

    private static final Pattern UUID_PATTERN =
        Pattern.compile("^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$");

    private final NamespacedKey key;
    private final double amount;
    private final Operation operation;
    private final EquipmentSlotGroup slot;

    @Deprecated public AttributeModifier(String name, double amount, Operation operation) {
        this(UUID.randomUUID(), name, amount, operation);
    }

    @Deprecated public AttributeModifier(UUID uuid, String name, double amount, Operation operation) {
        this(uuid, name, amount, operation, (EquipmentSlot) null);
    }

    @Deprecated public AttributeModifier(UUID uuid, String name, double amount, Operation operation, EquipmentSlot slot) {
        this(uuid, name, amount, operation, slot == null ? EquipmentSlotGroup.ANY : groupOf(slot));
    }

    /** A UUID-identified modifier becomes the key {@code minecraft:<uuid>}, as vanilla migrates them. */
    @Deprecated public AttributeModifier(UUID uuid, String name, double amount, Operation operation, EquipmentSlotGroup slot) {
        this(NamespacedKey.minecraft(uuid.toString()), amount, operation, slot);
    }

    public AttributeModifier(NamespacedKey key, double amount, Operation operation) {
        this(key, amount, operation, EquipmentSlotGroup.ANY);
    }

    public AttributeModifier(NamespacedKey key, double amount, Operation operation, EquipmentSlotGroup slot) {
        if (key == null) throw new IllegalArgumentException("Key cannot be null");
        if (operation == null) throw new IllegalArgumentException("Operation cannot be null");
        if (slot == null) throw new IllegalArgumentException("EquipmentSlotGroup cannot be null");
        this.key = key;
        this.amount = amount;
        this.operation = operation;
        this.slot = slot;
    }

    private static EquipmentSlotGroup groupOf(EquipmentSlot slot) {
        EquipmentSlotGroup group = EquipmentSlotGroup.getByName(slot.name());
        return group == null ? EquipmentSlotGroup.ANY : group;
    }

    /** The UUID a pre-1.21 modifier was keyed by, or one derived from the key. */
    @Deprecated public UUID getUniqueId() {
        if (key.getNamespace().equals("minecraft") && UUID_PATTERN.matcher(key.getKey()).matches())
            return UUID.fromString(key.getKey());
        return UUID.nameUUIDFromBytes(key.toString().getBytes(StandardCharsets.UTF_8));
    }

    @Override public NamespacedKey getKey() { return key; }

    public String getName() { return key.getKey(); }

    public double getAmount() { return amount; }

    public Operation getOperation() { return operation; }

    /** The single slot this modifier is limited to, or null when it spans a group. */
    @Deprecated public EquipmentSlot getSlot() {
        if (slot == EquipmentSlotGroup.ANY) return null;
        try { return EquipmentSlot.valueOf(slot.name()); }
        catch (IllegalArgumentException ignored) { return null; }
    }

    public EquipmentSlotGroup getSlotGroup() { return slot; }

    @Override public java.util.Map<String, Object> serialize() {
        java.util.Map<String, Object> data = new java.util.HashMap<>();
        data.put("key", key.toString());
        data.put("operation", operation.ordinal());
        data.put("amount", amount);
        if (slot != EquipmentSlotGroup.ANY) data.put("slot", slot.name().toLowerCase(java.util.Locale.ROOT));
        return data;
    }

    @Override public boolean equals(Object other) {
        return other instanceof AttributeModifier modifier && key.equals(modifier.key)
            && amount == modifier.amount && operation == modifier.operation && slot == modifier.slot;
    }

    @Override public int hashCode() {
        return java.util.Objects.hash(key, amount, operation, slot);
    }

    @Override public String toString() {
        return "AttributeModifier{key=" + key + ", operation=" + operation.name() + ", amount=" + amount
            + ", slot=" + slot.name().toLowerCase(java.util.Locale.ROOT) + "}";
    }

    public static AttributeModifier deserialize(java.util.Map<String, Object> args) {
        if (args == null) throw new IllegalArgumentException("args");
        Object id = args.containsKey("uuid") ? args.get("uuid") : args.get("key");
        NamespacedKey key = id == null ? null : NamespacedKey.fromString(String.valueOf(id));
        if (key == null) throw new IllegalArgumentException("Invalid attribute modifier key");
        Object amount = args.get("amount");
        Object operation = args.get("operation");
        if (!(amount instanceof Number number) || operation == null)
            throw new IllegalArgumentException("Invalid attribute modifier");
        Operation op = operation instanceof Number index ? Operation.values()[index.intValue()]
            : Operation.valueOf(String.valueOf(operation));
        EquipmentSlotGroup group = EquipmentSlotGroup.ANY;
        if (args.containsKey("slot")) {
            EquipmentSlotGroup named = EquipmentSlotGroup.getByName(String.valueOf(args.get("slot")));
            if (named != null) group = named;
        }
        return new AttributeModifier(key, number.doubleValue(), op, group);
    }
}
