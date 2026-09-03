package org.bukkit.attribute;

/** Snapshot of an entity attribute. */
public final class AttributeInstance {
    private final String entity;
    private final Attribute attribute;
    private final double base;
    private final double value;
    public AttributeInstance(Attribute attribute, double base, double value) {
        this(null, attribute, base, value);
    }
    public AttributeInstance(String entity, Attribute attribute, double base, double value) {
        this.entity = entity;
        this.attribute = attribute; this.base = base; this.value = value;
    }
    public Attribute getAttribute() { return attribute; }
    public double getBaseValue() { return base; }
    public double getValue() { return value; }
    public void setBaseValue(double value) { if (entity != null) foton.Native.setAttributeBase(entity, attribute.name(), value); }
    public java.util.Collection<AttributeModifier> getModifiers() {
        if (entity == null) return java.util.List.of();
        String[] encoded = foton.Native.attributeModifiers(entity, attribute.name());
        if (encoded == null) return java.util.List.of();
        java.util.ArrayList<AttributeModifier> result = new java.util.ArrayList<>();
        for (String item : encoded) {
            String[] fields = item.split("\\|", -1);
            if (fields.length != 4) continue;
            try { result.add(new AttributeModifier(java.util.UUID.fromString(fields[0]), fields[1],
                Double.parseDouble(fields[2]), AttributeModifier.Operation.valueOf(fields[3]))); }
            catch (IllegalArgumentException ignored) { }
        }
        return java.util.Collections.unmodifiableList(result);
    }
    public void addModifier(AttributeModifier modifier) {
        if (entity != null && modifier != null) foton.Native.addAttributeModifier(entity, attribute.name(),
            modifier.getUniqueId().toString(), modifier.getAmount(), modifier.getOperation().name());
    }
    public void removeModifier(AttributeModifier modifier) {
        if (entity != null && modifier != null) foton.Native.removeAttributeModifier(entity, attribute.name(), modifier.getUniqueId().toString());
    }
}
