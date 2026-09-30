package org.bukkit.attribute;

/** Minimal Paper 26.2 Attribute ABI fixture. */
public interface Attribute extends org.bukkit.util.OldEnum<Attribute>, org.bukkit.Keyed,
        org.bukkit.Translatable, net.kyori.adventure.translation.Translatable {
    Attribute MAX_HEALTH = null;
    Attribute FOLLOW_RANGE = null;
    Attribute KNOCKBACK_RESISTANCE = null;
    Attribute MOVEMENT_SPEED = null;
    Attribute FLYING_SPEED = null;
    Attribute ATTACK_DAMAGE = null;
    Attribute ATTACK_KNOCKBACK = null;
    Attribute ATTACK_SPEED = null;
    Attribute ARMOR = null;
    Attribute ARMOR_TOUGHNESS = null;
    Attribute FALL_DAMAGE_MULTIPLIER = null;
    Attribute LUCK = null;
    Attribute MAX_ABSORPTION = null;
    Attribute SAFE_FALL_DISTANCE = null;
    Attribute SCALE = null;
    Attribute STEP_HEIGHT = null;
    Attribute GRAVITY = null;
    Attribute JUMP_STRENGTH = null;
    Attribute BURNING_TIME = null;
    Attribute CAMERA_DISTANCE = null;
    Attribute EXPLOSION_KNOCKBACK_RESISTANCE = null;
    Attribute MOVEMENT_EFFICIENCY = null;
    Attribute OXYGEN_BONUS = null;
    Attribute WATER_MOVEMENT_EFFICIENCY = null;
    Attribute TEMPT_RANGE = null;
    Attribute BLOCK_INTERACTION_RANGE = null;
    Attribute ENTITY_INTERACTION_RANGE = null;
    Attribute BLOCK_BREAK_SPEED = null;
    Attribute MINING_EFFICIENCY = null;
    Attribute SNEAKING_SPEED = null;
    Attribute SUBMERGED_MINING_SPEED = null;
    Attribute SWEEPING_DAMAGE_RATIO = null;
    Attribute SPAWN_REINFORCEMENTS = null;
    Attribute WAYPOINT_TRANSMIT_RANGE = null;
    Attribute WAYPOINT_RECEIVE_RANGE = null;
    Attribute AIR_DRAG_MODIFIER = null;
    Attribute FRICTION_MODIFIER = null;
    Attribute BOUNCINESS = null;
    Attribute BELOW_NAME_DISTANCE = null;
    Attribute NAME_TAG_DISTANCE = null;

    Sentiment getSentiment();
    double getDefaultValue();

    static Attribute valueOf(String name) {
        return null;
    }

    static Attribute[] values() {
        return null;
    }

    enum Sentiment {
        POSITIVE,
        NEUTRAL,
        NEGATIVE
    }
}
