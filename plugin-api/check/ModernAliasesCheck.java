/** Modern Bukkit names must preserve the identity of Foton's canonical values. */
final class ModernAliasesCheck {
    private ModernAliasesCheck() {}

    static void check() throws ReflectiveOperationException {
        attributeAliasesPreserveCanonicalIdentity();
        potionAliasesPreserveCanonicalIdentity();
    }

    private static void attributeAliasesPreserveCanonicalIdentity() {
        assertAttributeAlias(org.bukkit.attribute.Attribute.ARMOR,
            org.bukkit.attribute.Attribute.GENERIC_ARMOR, "minecraft:armor");
        assertAttributeAlias(org.bukkit.attribute.Attribute.ARMOR_TOUGHNESS,
            org.bukkit.attribute.Attribute.GENERIC_ARMOR_TOUGHNESS,
            "minecraft:armor_toughness");
        assertAttributeAlias(org.bukkit.attribute.Attribute.KNOCKBACK_RESISTANCE,
            org.bukkit.attribute.Attribute.GENERIC_KNOCKBACK_RESISTANCE,
            "minecraft:knockback_resistance");
        assertAttributeAlias(org.bukkit.attribute.Attribute.MOVEMENT_SPEED,
            org.bukkit.attribute.Attribute.GENERIC_MOVEMENT_SPEED,
            "minecraft:movement_speed");
        assertAttributeAlias(org.bukkit.attribute.Attribute.SCALE,
            org.bukkit.attribute.Attribute.GENERIC_SCALE, "minecraft:scale");

        java.util.Map<org.bukkit.attribute.Attribute, String> expectedKeys =
            java.util.Map.ofEntries(
                java.util.Map.entry(org.bukkit.attribute.Attribute.GRAVITY,
                    "minecraft:gravity"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.GENERIC_MAX_HEALTH,
                    "minecraft:max_health"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.GENERIC_FOLLOW_RANGE,
                    "minecraft:follow_range"),
                java.util.Map.entry(
                    org.bukkit.attribute.Attribute.GENERIC_KNOCKBACK_RESISTANCE,
                    "minecraft:knockback_resistance"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.GENERIC_MOVEMENT_SPEED,
                    "minecraft:movement_speed"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.GENERIC_ATTACK_DAMAGE,
                    "minecraft:attack_damage"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.GENERIC_ATTACK_KNOCKBACK,
                    "minecraft:attack_knockback"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.GENERIC_ATTACK_SPEED,
                    "minecraft:attack_speed"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.GENERIC_ARMOR,
                    "minecraft:armor"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.GENERIC_ARMOR_TOUGHNESS,
                    "minecraft:armor_toughness"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.GENERIC_LUCK,
                    "minecraft:luck"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.GENERIC_JUMP_STRENGTH,
                    "minecraft:jump_strength"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.GENERIC_SCALE,
                    "minecraft:scale"),
                java.util.Map.entry(
                    org.bukkit.attribute.Attribute.PLAYER_BLOCK_INTERACTION_RANGE,
                    "minecraft:block_interaction_range"),
                java.util.Map.entry(
                    org.bukkit.attribute.Attribute.PLAYER_ENTITY_INTERACTION_RANGE,
                    "minecraft:entity_interaction_range"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.PLAYER_BLOCK_BREAK_SPEED,
                    "minecraft:block_break_speed"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.PLAYER_MINING_EFFICIENCY,
                    "minecraft:mining_efficiency"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.PLAYER_SNEAKING_SPEED,
                    "minecraft:sneaking_speed"),
                java.util.Map.entry(
                    org.bukkit.attribute.Attribute.ZOMBIE_SPAWN_REINFORCEMENTS,
                    "minecraft:spawn_reinforcements"));

        java.util.Set<org.bukkit.NamespacedKey> keys = new java.util.HashSet<>();
        java.util.Set<org.bukkit.attribute.Attribute> values =
            java.util.Collections.newSetFromMap(new java.util.IdentityHashMap<>());
        for (org.bukkit.attribute.Attribute attribute
                : org.bukkit.Registry.ATTRIBUTE) {
            Checks.same(attribute.getKey().toString(), expectedKeys.get(attribute),
                "Minecraft 26.2 attribute key for " + attribute);
            Checks.expect(keys.add(attribute.getKey()),
                "attribute registry contains duplicate key " + attribute.getKey());
            Checks.expect(values.add(attribute),
                "attribute registry contains a duplicate value " + attribute);
            Checks.expect(org.bukkit.Registry.ATTRIBUTE.get(attribute.getKey()) == attribute,
                "attribute registry lookup should preserve canonical identity for "
                    + attribute.getKey());
        }
        Checks.same(expectedKeys.size(), org.bukkit.attribute.Attribute.values().length,
            "attribute key fixture should cover every enum value");
        Checks.same(values.size(), org.bukkit.attribute.Attribute.values().length,
            "attribute aliases should not add enum or registry values");
    }

    private static void assertAttributeAlias(org.bukkit.attribute.Attribute alias,
            org.bukkit.attribute.Attribute canonical, String expectedKey) {
        Checks.expect(alias == canonical,
            "modern attribute alias should preserve canonical identity for " + canonical);
        Checks.same(alias.getKey().toString(), expectedKey,
            "modern attribute alias should expose its Minecraft 26.2 key");
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
