package foton;

import java.util.UUID;
import org.bukkit.NamespacedKey;
import org.bukkit.attribute.Attribute;
import org.bukkit.attribute.AttributeInstance;
import org.bukkit.attribute.AttributeModifier;

/** An entity's attribute, read and written through Foton on every call.
 *
 * <p>Live rather than a snapshot, as Paper's is: a plugin that keeps the
 * instance and reads {@link #getValue()} a second later sees the modifiers that
 * potions, armour and other plugins applied in between. Once the entity is gone
 * reads answer the registry default and writes do nothing. */
final class FotonAttributeInstance implements AttributeInstance {
    private final String entity;
    private final Attribute attribute;

    private FotonAttributeInstance(String entity, Attribute attribute) {
        this.entity = entity;
        this.attribute = attribute;
    }

    /** The instance, or null when the entity does not have this attribute at all. */
    static AttributeInstance of(UUID entity, Attribute attribute) {
        if (entity == null || attribute == null) return null;
        String id = entity.toString();
        return Native.attributeValues(id, attribute.getKey().toString()) == null
            ? null : new FotonAttributeInstance(id, attribute);
    }

    private double field(int index) {
        double[] values = Native.attributeValues(entity, attribute.getKey().toString());
        return values == null || values.length < 3 ? 0.0 : values[index];
    }

    @Override public Attribute getAttribute() { return attribute; }
    @Override public double getBaseValue() { return field(0); }
    @Override public double getValue() { return field(1); }
    @Override public double getDefaultValue() { return field(2); }

    @Override public void setBaseValue(double value) {
        Native.setAttributeBase(entity, attribute.getKey().toString(), value);
    }

    @Override public java.util.Collection<AttributeModifier> getModifiers() {
        String[] encoded = Native.attributeModifierList(entity, attribute.getKey().toString());
        if (encoded == null) return java.util.List.of();
        java.util.ArrayList<AttributeModifier> result = new java.util.ArrayList<>(encoded.length);
        for (String item : encoded) {
            String[] fields = item.split("\\|", -1);
            if (fields.length != 3) continue;
            NamespacedKey key = NamespacedKey.fromString(fields[0]);
            if (key == null) continue;
            try {
                result.add(new AttributeModifier(key, Double.parseDouble(fields[1]),
                    AttributeModifier.Operation.valueOf(fields[2])));
            } catch (IllegalArgumentException ignored) { }
        }
        return java.util.Collections.unmodifiableList(result);
    }

    private void add(AttributeModifier modifier, boolean persistent) {
        if (modifier == null) throw new IllegalArgumentException("modifier");
        // Vanilla refuses a second modifier under a key already present, and so
        // does Paper -- loudly, with an exception the plugin can see.
        if (!Native.addAttributeModifierKeyed(entity, attribute.getKey().toString(),
                modifier.getKey().toString(), modifier.getAmount(), modifier.getOperation().name(), persistent)
                && Native.attributeValues(entity, attribute.getKey().toString()) != null)
            throw new IllegalArgumentException("Modifier is already applied on this attribute!");
    }

    @Override public void addModifier(AttributeModifier modifier) { add(modifier, true); }
    @Override public void addTransientModifier(AttributeModifier modifier) { add(modifier, false); }

    @Override public void removeModifier(AttributeModifier modifier) {
        if (modifier != null) removeModifier(modifier.getKey());
    }

    @Override public void removeModifier(net.kyori.adventure.key.Key key) {
        if (key != null) Native.removeAttributeModifierKeyed(entity, attribute.getKey().toString(), key.asString());
    }

    @Override public void removeModifier(UUID uuid) {
        if (uuid != null) removeModifier(NamespacedKey.minecraft(uuid.toString()));
    }

    @Override public String toString() { return "FotonAttributeInstance{" + attribute.getKey() + "}"; }
}
