package org.bukkit.attribute;

/** Vanilla attribute keys commonly used by Bukkit plugins. */
public enum Attribute implements org.bukkit.Keyed {
    GRAVITY,
    GENERIC_MAX_HEALTH, GENERIC_FOLLOW_RANGE, GENERIC_KNOCKBACK_RESISTANCE,
    GENERIC_MOVEMENT_SPEED, GENERIC_ATTACK_DAMAGE, GENERIC_ATTACK_KNOCKBACK,
    GENERIC_ATTACK_SPEED, GENERIC_ARMOR, GENERIC_ARMOR_TOUGHNESS,
    GENERIC_LUCK, GENERIC_JUMP_STRENGTH, GENERIC_SCALE,
    PLAYER_BLOCK_INTERACTION_RANGE, PLAYER_ENTITY_INTERACTION_RANGE,
    PLAYER_BLOCK_BREAK_SPEED, PLAYER_MINING_EFFICIENCY, PLAYER_SNEAKING_SPEED,
    ZOMBIE_SPAWN_REINFORCEMENTS;

    /** Paper/Bukkit compatibility alias. */
    public static final Attribute MAX_HEALTH = GENERIC_MAX_HEALTH;

    @Override public org.bukkit.NamespacedKey getKey() { return org.bukkit.NamespacedKey.minecraft(name().toLowerCase(java.util.Locale.ROOT).replace('_', '.')); }
}
