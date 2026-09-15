package org.bukkit.attribute;

/** Registry-backed Paper 26.2 attribute surface. */
public interface Attribute extends org.bukkit.util.OldEnum<Attribute>, org.bukkit.Keyed,
        org.bukkit.Translatable, net.kyori.adventure.translation.Translatable {
    Attribute MAX_HEALTH = FotonAttributes.MAX_HEALTH;
    Attribute FOLLOW_RANGE = FotonAttributes.FOLLOW_RANGE;
    Attribute KNOCKBACK_RESISTANCE = FotonAttributes.KNOCKBACK_RESISTANCE;
    Attribute MOVEMENT_SPEED = FotonAttributes.MOVEMENT_SPEED;
    Attribute FLYING_SPEED = FotonAttributes.FLYING_SPEED;
    Attribute ATTACK_DAMAGE = FotonAttributes.ATTACK_DAMAGE;
    Attribute ATTACK_KNOCKBACK = FotonAttributes.ATTACK_KNOCKBACK;
    Attribute ATTACK_SPEED = FotonAttributes.ATTACK_SPEED;
    Attribute ARMOR = FotonAttributes.ARMOR;
    Attribute ARMOR_TOUGHNESS = FotonAttributes.ARMOR_TOUGHNESS;
    Attribute FALL_DAMAGE_MULTIPLIER = FotonAttributes.FALL_DAMAGE_MULTIPLIER;
    Attribute LUCK = FotonAttributes.LUCK;
    Attribute MAX_ABSORPTION = FotonAttributes.MAX_ABSORPTION;
    Attribute SAFE_FALL_DISTANCE = FotonAttributes.SAFE_FALL_DISTANCE;
    Attribute SCALE = FotonAttributes.SCALE;
    Attribute STEP_HEIGHT = FotonAttributes.STEP_HEIGHT;
    Attribute GRAVITY = FotonAttributes.GRAVITY;
    Attribute JUMP_STRENGTH = FotonAttributes.JUMP_STRENGTH;
    Attribute BURNING_TIME = FotonAttributes.BURNING_TIME;
    Attribute CAMERA_DISTANCE = FotonAttributes.CAMERA_DISTANCE;
    Attribute EXPLOSION_KNOCKBACK_RESISTANCE =
        FotonAttributes.EXPLOSION_KNOCKBACK_RESISTANCE;
    Attribute MOVEMENT_EFFICIENCY = FotonAttributes.MOVEMENT_EFFICIENCY;
    Attribute OXYGEN_BONUS = FotonAttributes.OXYGEN_BONUS;
    Attribute WATER_MOVEMENT_EFFICIENCY = FotonAttributes.WATER_MOVEMENT_EFFICIENCY;
    Attribute TEMPT_RANGE = FotonAttributes.TEMPT_RANGE;
    Attribute BLOCK_INTERACTION_RANGE = FotonAttributes.BLOCK_INTERACTION_RANGE;
    Attribute ENTITY_INTERACTION_RANGE = FotonAttributes.ENTITY_INTERACTION_RANGE;
    Attribute BLOCK_BREAK_SPEED = FotonAttributes.BLOCK_BREAK_SPEED;
    Attribute MINING_EFFICIENCY = FotonAttributes.MINING_EFFICIENCY;
    Attribute SNEAKING_SPEED = FotonAttributes.SNEAKING_SPEED;
    Attribute SUBMERGED_MINING_SPEED = FotonAttributes.SUBMERGED_MINING_SPEED;
    Attribute SWEEPING_DAMAGE_RATIO = FotonAttributes.SWEEPING_DAMAGE_RATIO;
    Attribute SPAWN_REINFORCEMENTS = FotonAttributes.SPAWN_REINFORCEMENTS;
    Attribute WAYPOINT_TRANSMIT_RANGE = FotonAttributes.WAYPOINT_TRANSMIT_RANGE;
    Attribute WAYPOINT_RECEIVE_RANGE = FotonAttributes.WAYPOINT_RECEIVE_RANGE;
    Attribute AIR_DRAG_MODIFIER = FotonAttributes.AIR_DRAG_MODIFIER;
    Attribute FRICTION_MODIFIER = FotonAttributes.FRICTION_MODIFIER;
    Attribute BOUNCINESS = FotonAttributes.BOUNCINESS;
    Attribute BELOW_NAME_DISTANCE = FotonAttributes.BELOW_NAME_DISTANCE;
    Attribute NAME_TAG_DISTANCE = FotonAttributes.NAME_TAG_DISTANCE;

    /** Legacy Bukkit names retained as aliases of the modern registry values. */
    Attribute GENERIC_MAX_HEALTH = MAX_HEALTH;
    Attribute GENERIC_FOLLOW_RANGE = FOLLOW_RANGE;
    Attribute GENERIC_KNOCKBACK_RESISTANCE = KNOCKBACK_RESISTANCE;
    Attribute GENERIC_MOVEMENT_SPEED = MOVEMENT_SPEED;
    Attribute GENERIC_ATTACK_DAMAGE = ATTACK_DAMAGE;
    Attribute GENERIC_ATTACK_KNOCKBACK = ATTACK_KNOCKBACK;
    Attribute GENERIC_ATTACK_SPEED = ATTACK_SPEED;
    Attribute GENERIC_ARMOR = ARMOR;
    Attribute GENERIC_ARMOR_TOUGHNESS = ARMOR_TOUGHNESS;
    Attribute GENERIC_LUCK = LUCK;
    Attribute GENERIC_JUMP_STRENGTH = JUMP_STRENGTH;
    Attribute GENERIC_SCALE = SCALE;
    Attribute PLAYER_BLOCK_INTERACTION_RANGE = BLOCK_INTERACTION_RANGE;
    Attribute PLAYER_ENTITY_INTERACTION_RANGE = ENTITY_INTERACTION_RANGE;
    Attribute PLAYER_BLOCK_BREAK_SPEED = BLOCK_BREAK_SPEED;
    Attribute PLAYER_MINING_EFFICIENCY = MINING_EFFICIENCY;
    Attribute PLAYER_SNEAKING_SPEED = SNEAKING_SPEED;
    Attribute ZOMBIE_SPAWN_REINFORCEMENTS = SPAWN_REINFORCEMENTS;

    Sentiment getSentiment();

    double getDefaultValue();

    static Attribute valueOf(String name) {
        return FotonAttributes.valueOf(name);
    }

    static Attribute[] values() {
        return FotonAttributes.values();
    }

    enum Sentiment {
        POSITIVE,
        NEUTRAL,
        NEGATIVE
    }
}

final class FotonAttribute implements Attribute {
    private final org.bukkit.NamespacedKey key;
    private final String name;
    private final String translationKey;
    private final int ordinal;
    private final double defaultValue;
    private final Attribute.Sentiment sentiment;

    FotonAttribute(String key, String translationKey, int ordinal, double defaultValue,
            Attribute.Sentiment sentiment) {
        this.key = org.bukkit.NamespacedKey.minecraft(key);
        this.name = key.toUpperCase(java.util.Locale.ROOT);
        this.translationKey = translationKey;
        this.ordinal = ordinal;
        this.defaultValue = defaultValue;
        this.sentiment = sentiment;
    }

    @Override public org.bukkit.NamespacedKey getKey() { return key; }
    @Override public String getTranslationKey() { return translationKey; }
    @Override public String translationKey() { return translationKey; }
    @Override public Attribute.Sentiment getSentiment() { return sentiment; }
    @Override public double getDefaultValue() { return defaultValue; }
    @Override public String name() { return name; }
    @Override public int ordinal() { return ordinal; }
    @Override public int compareTo(Attribute other) {
        return Integer.compare(ordinal, other.ordinal());
    }
    @Override public String toString() { return name; }
}

final class FotonAttributes {
    static final Attribute AIR_DRAG_MODIFIER = value(
        "air_drag_modifier", "attribute.name.air_drag_modifier", 0, 1.0);
    static final Attribute ARMOR = value("armor", "attribute.name.armor", 1, 0.0);
    static final Attribute ARMOR_TOUGHNESS = value(
        "armor_toughness", "attribute.name.armor_toughness", 2, 0.0);
    static final Attribute ATTACK_DAMAGE = value(
        "attack_damage", "attribute.name.attack_damage", 3, 2.0);
    static final Attribute ATTACK_KNOCKBACK = value(
        "attack_knockback", "attribute.name.attack_knockback", 4, 0.0);
    static final Attribute ATTACK_SPEED = value(
        "attack_speed", "attribute.name.attack_speed", 5, 4.0);
    static final Attribute BELOW_NAME_DISTANCE = value(
        "below_name_distance", "attribute.name.below_name_distance", 6, 10.0);
    static final Attribute BLOCK_BREAK_SPEED = value(
        "block_break_speed", "attribute.name.block_break_speed", 7, 1.0);
    static final Attribute BLOCK_INTERACTION_RANGE = value(
        "block_interaction_range", "attribute.name.block_interaction_range", 8, 4.5);
    static final Attribute BOUNCINESS = value(
        "bounciness", "attribute.name.bounciness", 9, 0.0);
    static final Attribute BURNING_TIME = value(
        "burning_time", "attribute.name.burning_time", 10, 1.0,
        Attribute.Sentiment.NEGATIVE);
    static final Attribute CAMERA_DISTANCE = value(
        "camera_distance", "attribute.name.camera_distance", 11, 4.0);
    static final Attribute EXPLOSION_KNOCKBACK_RESISTANCE = value(
        "explosion_knockback_resistance",
        "attribute.name.explosion_knockback_resistance", 12, 0.0);
    static final Attribute ENTITY_INTERACTION_RANGE = value(
        "entity_interaction_range", "attribute.name.entity_interaction_range", 13, 3.0);
    static final Attribute FALL_DAMAGE_MULTIPLIER = value(
        "fall_damage_multiplier", "attribute.name.fall_damage_multiplier", 14, 1.0,
        Attribute.Sentiment.NEGATIVE);
    static final Attribute FLYING_SPEED = value(
        "flying_speed", "attribute.name.flying_speed", 15, 0.4);
    static final Attribute FOLLOW_RANGE = value(
        "follow_range", "attribute.name.follow_range", 16, 32.0);
    static final Attribute FRICTION_MODIFIER = value(
        "friction_modifier", "attribute.name.friction_modifier", 17, 1.0);
    static final Attribute GRAVITY = value(
        "gravity", "attribute.name.gravity", 18, 0.08, Attribute.Sentiment.NEUTRAL);
    static final Attribute JUMP_STRENGTH = value(
        "jump_strength", "attribute.name.jump_strength", 19, 0.41999998688697815);
    static final Attribute KNOCKBACK_RESISTANCE = value(
        "knockback_resistance", "attribute.name.knockback_resistance", 20, 0.0);
    static final Attribute LUCK = value("luck", "attribute.name.luck", 21, 0.0);
    static final Attribute MAX_ABSORPTION = value(
        "max_absorption", "attribute.name.max_absorption", 22, 0.0);
    static final Attribute MAX_HEALTH = value(
        "max_health", "attribute.name.max_health", 23, 20.0);
    static final Attribute MINING_EFFICIENCY = value(
        "mining_efficiency", "attribute.name.mining_efficiency", 24, 0.0);
    static final Attribute MOVEMENT_EFFICIENCY = value(
        "movement_efficiency", "attribute.name.movement_efficiency", 25, 0.0);
    static final Attribute MOVEMENT_SPEED = value(
        "movement_speed", "attribute.name.movement_speed", 26, 0.7);
    static final Attribute NAME_TAG_DISTANCE = value(
        "name_tag_distance", "attribute.name.name_tag_distance", 27, 64.0);
    static final Attribute OXYGEN_BONUS = value(
        "oxygen_bonus", "attribute.name.oxygen_bonus", 28, 0.0);
    static final Attribute SAFE_FALL_DISTANCE = value(
        "safe_fall_distance", "attribute.name.safe_fall_distance", 29, 3.0);
    static final Attribute SCALE = value(
        "scale", "attribute.name.scale", 30, 1.0, Attribute.Sentiment.NEUTRAL);
    static final Attribute SNEAKING_SPEED = value(
        "sneaking_speed", "attribute.name.sneaking_speed", 31, 0.3);
    static final Attribute SPAWN_REINFORCEMENTS = value(
        "spawn_reinforcements", "attribute.name.spawn_reinforcements", 32, 0.0);
    static final Attribute STEP_HEIGHT = value(
        "step_height", "attribute.name.step_height", 33, 0.6);
    static final Attribute SUBMERGED_MINING_SPEED = value(
        "submerged_mining_speed", "attribute.name.submerged_mining_speed", 34, 0.2);
    static final Attribute SWEEPING_DAMAGE_RATIO = value(
        "sweeping_damage_ratio", "attribute.name.sweeping_damage_ratio", 35, 0.0);
    static final Attribute TEMPT_RANGE = value(
        "tempt_range", "attribute.name.tempt_range", 36, 10.0);
    static final Attribute WATER_MOVEMENT_EFFICIENCY = value(
        "water_movement_efficiency", "attribute.name.water_movement_efficiency", 37, 0.0);
    static final Attribute WAYPOINT_TRANSMIT_RANGE = value(
        "waypoint_transmit_range", "attribute.name.waypoint_transmit_range", 38, 0.0,
        Attribute.Sentiment.NEUTRAL);
    static final Attribute WAYPOINT_RECEIVE_RANGE = value(
        "waypoint_receive_range", "attribute.name.waypoint_receive_range", 39, 0.0,
        Attribute.Sentiment.NEUTRAL);

    private static final Attribute[] VALUES = {
        AIR_DRAG_MODIFIER, ARMOR, ARMOR_TOUGHNESS, ATTACK_DAMAGE, ATTACK_KNOCKBACK,
        ATTACK_SPEED, BELOW_NAME_DISTANCE, BLOCK_BREAK_SPEED, BLOCK_INTERACTION_RANGE,
        BOUNCINESS, BURNING_TIME, CAMERA_DISTANCE, EXPLOSION_KNOCKBACK_RESISTANCE,
        ENTITY_INTERACTION_RANGE, FALL_DAMAGE_MULTIPLIER, FLYING_SPEED, FOLLOW_RANGE,
        FRICTION_MODIFIER, GRAVITY, JUMP_STRENGTH, KNOCKBACK_RESISTANCE, LUCK,
        MAX_ABSORPTION, MAX_HEALTH, MINING_EFFICIENCY, MOVEMENT_EFFICIENCY,
        MOVEMENT_SPEED, NAME_TAG_DISTANCE, OXYGEN_BONUS, SAFE_FALL_DISTANCE, SCALE,
        SNEAKING_SPEED, SPAWN_REINFORCEMENTS, STEP_HEIGHT, SUBMERGED_MINING_SPEED,
        SWEEPING_DAMAGE_RATIO, TEMPT_RANGE, WATER_MOVEMENT_EFFICIENCY,
        WAYPOINT_TRANSMIT_RANGE, WAYPOINT_RECEIVE_RANGE
    };
    private static final java.util.Map<String, Attribute> BY_NAME = names();

    private FotonAttributes() {}

    static Attribute valueOf(String name) {
        String normalized = java.util.Objects.requireNonNull(name, "name")
            .toUpperCase(java.util.Locale.ROOT);
        if (normalized.startsWith("MINECRAFT:")) {
            normalized = normalized.substring("MINECRAFT:".length());
        }
        Attribute value = BY_NAME.get(normalized);
        if (value == null) {
            throw new IllegalArgumentException("No attribute found with the name " + name);
        }
        return value;
    }

    static Attribute[] values() {
        return java.util.Arrays.copyOf(VALUES, VALUES.length);
    }

    private static Attribute value(String key, String translationKey, int ordinal,
            double defaultValue) {
        return value(key, translationKey, ordinal, defaultValue, Attribute.Sentiment.POSITIVE);
    }

    private static Attribute value(String key, String translationKey, int ordinal,
            double defaultValue, Attribute.Sentiment sentiment) {
        return new FotonAttribute(key, translationKey, ordinal, defaultValue, sentiment);
    }

    private static java.util.Map<String, Attribute> names() {
        java.util.Map<String, Attribute> values = new java.util.HashMap<>();
        for (Attribute value : VALUES) values.put(value.name(), value);
        values.put("GENERIC_MAX_HEALTH", MAX_HEALTH);
        values.put("GENERIC_FOLLOW_RANGE", FOLLOW_RANGE);
        values.put("GENERIC_KNOCKBACK_RESISTANCE", KNOCKBACK_RESISTANCE);
        values.put("GENERIC_MOVEMENT_SPEED", MOVEMENT_SPEED);
        values.put("GENERIC_ATTACK_DAMAGE", ATTACK_DAMAGE);
        values.put("GENERIC_ATTACK_KNOCKBACK", ATTACK_KNOCKBACK);
        values.put("GENERIC_ATTACK_SPEED", ATTACK_SPEED);
        values.put("GENERIC_ARMOR", ARMOR);
        values.put("GENERIC_ARMOR_TOUGHNESS", ARMOR_TOUGHNESS);
        values.put("GENERIC_LUCK", LUCK);
        values.put("GENERIC_JUMP_STRENGTH", JUMP_STRENGTH);
        values.put("GENERIC_SCALE", SCALE);
        values.put("PLAYER_BLOCK_INTERACTION_RANGE", BLOCK_INTERACTION_RANGE);
        values.put("PLAYER_ENTITY_INTERACTION_RANGE", ENTITY_INTERACTION_RANGE);
        values.put("PLAYER_BLOCK_BREAK_SPEED", BLOCK_BREAK_SPEED);
        values.put("PLAYER_MINING_EFFICIENCY", MINING_EFFICIENCY);
        values.put("PLAYER_SNEAKING_SPEED", SNEAKING_SPEED);
        values.put("ZOMBIE_SPAWN_REINFORCEMENTS", SPAWN_REINFORCEMENTS);
        return java.util.Collections.unmodifiableMap(values);
    }
}
