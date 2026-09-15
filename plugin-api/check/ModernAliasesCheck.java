/** Modern Bukkit names must preserve the identity of Foton's canonical values. */
final class ModernAliasesCheck {
    private ModernAliasesCheck() {}

    static void check() throws ReflectiveOperationException {
        attributeAliasesPreserveCanonicalIdentity();
        potionAliasesPreserveCanonicalIdentity();
    }

    private static void attributeAliasesPreserveCanonicalIdentity() {
        assertAttributeAlias(org.bukkit.attribute.Attribute.ARMOR,
            org.bukkit.attribute.Attribute.GENERIC_ARMOR);
        assertAttributeAlias(org.bukkit.attribute.Attribute.ARMOR_TOUGHNESS,
            org.bukkit.attribute.Attribute.GENERIC_ARMOR_TOUGHNESS);
        assertAttributeAlias(org.bukkit.attribute.Attribute.KNOCKBACK_RESISTANCE,
            org.bukkit.attribute.Attribute.GENERIC_KNOCKBACK_RESISTANCE);
        assertAttributeAlias(org.bukkit.attribute.Attribute.MOVEMENT_SPEED,
            org.bukkit.attribute.Attribute.GENERIC_MOVEMENT_SPEED);
        assertAttributeAlias(org.bukkit.attribute.Attribute.SCALE,
            org.bukkit.attribute.Attribute.GENERIC_SCALE);

        java.util.Set<org.bukkit.NamespacedKey> keys = new java.util.HashSet<>();
        java.util.Set<org.bukkit.attribute.Attribute> values =
            java.util.Collections.newSetFromMap(new java.util.IdentityHashMap<>());
        for (org.bukkit.attribute.Attribute attribute
                : org.bukkit.Registry.ATTRIBUTE) {
            Checks.expect(keys.add(attribute.getKey()),
                "attribute registry contains duplicate key " + attribute.getKey());
            Checks.expect(values.add(attribute),
                "attribute registry contains a duplicate value " + attribute);
            Checks.expect(org.bukkit.Registry.ATTRIBUTE.get(attribute.getKey()) == attribute,
                "attribute registry lookup should preserve canonical identity for "
                    + attribute.getKey());
        }
        Checks.same(values.size(), org.bukkit.attribute.Attribute.values().length,
            "attribute aliases should not add enum or registry values");
    }

    private static void assertAttributeAlias(org.bukkit.attribute.Attribute alias,
            org.bukkit.attribute.Attribute canonical) {
        Checks.expect(alias == canonical,
            "modern attribute alias should preserve canonical identity for " + canonical);
        Checks.expect(org.bukkit.Registry.ATTRIBUTE.get(alias.getKey()) == canonical,
            "modern attribute key should resolve the canonical value for " + canonical);
    }

    private static void potionAliasesPreserveCanonicalIdentity()
            throws ReflectiveOperationException {
        assertPotionAlias(org.bukkit.potion.PotionEffectType.HASTE,
            org.bukkit.potion.PotionEffectType.FAST_DIGGING);
        assertPotionAlias(org.bukkit.potion.PotionEffectType.JUMP_BOOST,
            org.bukkit.potion.PotionEffectType.JUMP);
        assertPotionAlias(org.bukkit.potion.PotionEffectType.RESISTANCE,
            org.bukkit.potion.PotionEffectType.DAMAGE_RESISTANCE);
        assertPotionAlias(org.bukkit.potion.PotionEffectType.STRENGTH,
            org.bukkit.potion.PotionEffectType.INCREASE_DAMAGE);

        java.util.Map<String, org.bukkit.potion.PotionEffectType> canonicalByName =
            new java.util.HashMap<>();
        for (java.lang.reflect.Field field
                : org.bukkit.potion.PotionEffectType.class.getFields()) {
            if (field.getType() != org.bukkit.potion.PotionEffectType.class) continue;
            org.bukkit.potion.PotionEffectType value =
                (org.bukkit.potion.PotionEffectType) field.get(null);
            org.bukkit.potion.PotionEffectType existing =
                canonicalByName.putIfAbsent(value.getName(), value);
            Checks.expect(existing == null || existing == value,
                "potion effect fields contain duplicate objects for " + value.getName());
        }
    }

    private static void assertPotionAlias(org.bukkit.potion.PotionEffectType alias,
            org.bukkit.potion.PotionEffectType canonical) {
        Checks.expect(alias == canonical,
            "modern potion alias should preserve canonical identity for " + canonical);
    }
}
