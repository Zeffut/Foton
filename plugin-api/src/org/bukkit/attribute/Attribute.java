package org.bukkit.attribute;

/** Vanilla attribute keys commonly used by Bukkit plugins. */
public enum Attribute implements org.bukkit.Keyed {
    GRAVITY("gravity"),
    GENERIC_MAX_HEALTH("max_health"),
    GENERIC_FOLLOW_RANGE("follow_range"),
    GENERIC_KNOCKBACK_RESISTANCE("knockback_resistance"),
    GENERIC_MOVEMENT_SPEED("movement_speed"),
    GENERIC_ATTACK_DAMAGE("attack_damage"),
    GENERIC_ATTACK_KNOCKBACK("attack_knockback"),
    GENERIC_ATTACK_SPEED("attack_speed"),
    GENERIC_ARMOR("armor"),
    GENERIC_ARMOR_TOUGHNESS("armor_toughness"),
    GENERIC_LUCK("luck"),
    GENERIC_JUMP_STRENGTH("jump_strength"),
    GENERIC_SCALE("scale"),
    PLAYER_BLOCK_INTERACTION_RANGE("block_interaction_range"),
    PLAYER_ENTITY_INTERACTION_RANGE("entity_interaction_range"),
    PLAYER_BLOCK_BREAK_SPEED("block_break_speed"),
    PLAYER_MINING_EFFICIENCY("mining_efficiency"),
    PLAYER_SNEAKING_SPEED("sneaking_speed"),
    ZOMBIE_SPAWN_REINFORCEMENTS("spawn_reinforcements");

    private final org.bukkit.NamespacedKey key;

    Attribute(String key) {
        this.key = org.bukkit.NamespacedKey.minecraft(key);
    }

    /** Paper/Bukkit compatibility alias. */
    public static final Attribute MAX_HEALTH = GENERIC_MAX_HEALTH;
    public static final Attribute ARMOR = GENERIC_ARMOR;
    public static final Attribute ARMOR_TOUGHNESS = GENERIC_ARMOR_TOUGHNESS;
    public static final Attribute KNOCKBACK_RESISTANCE = GENERIC_KNOCKBACK_RESISTANCE;
    public static final Attribute MOVEMENT_SPEED = GENERIC_MOVEMENT_SPEED;
    public static final Attribute SCALE = GENERIC_SCALE;

    @Override public org.bukkit.NamespacedKey getKey() { return key; }
}
