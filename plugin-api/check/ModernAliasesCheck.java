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
                java.util.Map.entry(org.bukkit.attribute.Attribute.AIR_DRAG_MODIFIER,
                    "minecraft:air_drag_modifier"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.ARMOR,
                    "minecraft:armor"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.ARMOR_TOUGHNESS,
                    "minecraft:armor_toughness"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.ATTACK_DAMAGE,
                    "minecraft:attack_damage"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.ATTACK_KNOCKBACK,
                    "minecraft:attack_knockback"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.ATTACK_SPEED,
                    "minecraft:attack_speed"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.BELOW_NAME_DISTANCE,
                    "minecraft:below_name_distance"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.BLOCK_BREAK_SPEED,
                    "minecraft:block_break_speed"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.BLOCK_INTERACTION_RANGE,
                    "minecraft:block_interaction_range"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.BOUNCINESS,
                    "minecraft:bounciness"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.BURNING_TIME,
                    "minecraft:burning_time"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.CAMERA_DISTANCE,
                    "minecraft:camera_distance"),
                java.util.Map.entry(
                    org.bukkit.attribute.Attribute.EXPLOSION_KNOCKBACK_RESISTANCE,
                    "minecraft:explosion_knockback_resistance"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.ENTITY_INTERACTION_RANGE,
                    "minecraft:entity_interaction_range"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.FALL_DAMAGE_MULTIPLIER,
                    "minecraft:fall_damage_multiplier"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.FLYING_SPEED,
                    "minecraft:flying_speed"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.FOLLOW_RANGE,
                    "minecraft:follow_range"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.FRICTION_MODIFIER,
                    "minecraft:friction_modifier"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.GRAVITY,
                    "minecraft:gravity"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.JUMP_STRENGTH,
                    "minecraft:jump_strength"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.KNOCKBACK_RESISTANCE,
                    "minecraft:knockback_resistance"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.LUCK,
                    "minecraft:luck"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.MAX_ABSORPTION,
                    "minecraft:max_absorption"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.MAX_HEALTH,
                    "minecraft:max_health"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.MINING_EFFICIENCY,
                    "minecraft:mining_efficiency"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.MOVEMENT_EFFICIENCY,
                    "minecraft:movement_efficiency"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.MOVEMENT_SPEED,
                    "minecraft:movement_speed"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.NAME_TAG_DISTANCE,
                    "minecraft:name_tag_distance"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.OXYGEN_BONUS,
                    "minecraft:oxygen_bonus"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.SAFE_FALL_DISTANCE,
                    "minecraft:safe_fall_distance"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.SCALE,
                    "minecraft:scale"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.SNEAKING_SPEED,
                    "minecraft:sneaking_speed"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.SPAWN_REINFORCEMENTS,
                    "minecraft:spawn_reinforcements"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.STEP_HEIGHT,
                    "minecraft:step_height"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.SUBMERGED_MINING_SPEED,
                    "minecraft:submerged_mining_speed"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.SWEEPING_DAMAGE_RATIO,
                    "minecraft:sweeping_damage_ratio"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.TEMPT_RANGE,
                    "minecraft:tempt_range"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.WATER_MOVEMENT_EFFICIENCY,
                    "minecraft:water_movement_efficiency"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.WAYPOINT_TRANSMIT_RANGE,
                    "minecraft:waypoint_transmit_range"),
                java.util.Map.entry(org.bukkit.attribute.Attribute.WAYPOINT_RECEIVE_RANGE,
                    "minecraft:waypoint_receive_range"));

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
        java.util.Map<String, org.bukkit.potion.PotionEffectType> canonicalByName =
            new java.util.HashMap<>();
        for (java.lang.reflect.Field field
                : org.bukkit.potion.PotionEffectType.class.getFields()) {
            if (field.getType() != org.bukkit.potion.PotionEffectType.class) continue;
            org.bukkit.potion.PotionEffectType value =
                (org.bukkit.potion.PotionEffectType) field.get(null);
            String name = value.getKey().getKey();
            org.bukkit.potion.PotionEffectType existing = canonicalByName.putIfAbsent(name, value);
            Checks.expect(existing == null,
                "Paper 26.2 potion fields must be one-to-one for " + name);
            Checks.expect(org.bukkit.Registry.MOB_EFFECT.get(value.getKey()) == value,
                "mob effect registry must retain canonical identity for " + name);
        }
        Checks.same(canonicalByName.size(), 40,
            "Paper 26.2 potion constants must cover the full extracted registry");
    }
}
