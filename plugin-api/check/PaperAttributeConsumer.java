import org.bukkit.attribute.Attribute;

/** Compiled against Paper's Attribute interface and linked against Foton at runtime. */
public final class PaperAttributeConsumer {
    private PaperAttributeConsumer() {}

    public static void invokeInterface(Attribute attribute) {
        if (attribute.getKey() == null || attribute.key() == null) {
            throw new AssertionError("attribute keys must be available through both APIs");
        }
        if (attribute.getTranslationKey() == null || attribute.translationKey() == null) {
            throw new AssertionError("attribute translation keys must be available");
        }
        if (attribute.getSentiment() == null || attribute.getDefaultValue() < 0.0) {
            throw new AssertionError("attribute metadata must be available");
        }
        if (attribute.ordinal() < 0 || attribute.name() == null) {
            throw new AssertionError("OldEnum methods must preserve canonical ordering");
        }

        Attribute[] fields = {
            Attribute.MAX_HEALTH, Attribute.FOLLOW_RANGE,
            Attribute.KNOCKBACK_RESISTANCE, Attribute.MOVEMENT_SPEED,
            Attribute.FLYING_SPEED, Attribute.ATTACK_DAMAGE,
            Attribute.ATTACK_KNOCKBACK, Attribute.ATTACK_SPEED,
            Attribute.ARMOR, Attribute.ARMOR_TOUGHNESS,
            Attribute.FALL_DAMAGE_MULTIPLIER, Attribute.LUCK,
            Attribute.MAX_ABSORPTION, Attribute.SAFE_FALL_DISTANCE,
            Attribute.SCALE, Attribute.STEP_HEIGHT, Attribute.GRAVITY,
            Attribute.JUMP_STRENGTH, Attribute.BURNING_TIME,
            Attribute.CAMERA_DISTANCE, Attribute.EXPLOSION_KNOCKBACK_RESISTANCE,
            Attribute.MOVEMENT_EFFICIENCY, Attribute.OXYGEN_BONUS,
            Attribute.WATER_MOVEMENT_EFFICIENCY, Attribute.TEMPT_RANGE,
            Attribute.BLOCK_INTERACTION_RANGE, Attribute.ENTITY_INTERACTION_RANGE,
            Attribute.BLOCK_BREAK_SPEED, Attribute.MINING_EFFICIENCY,
            Attribute.SNEAKING_SPEED, Attribute.SUBMERGED_MINING_SPEED,
            Attribute.SWEEPING_DAMAGE_RATIO, Attribute.SPAWN_REINFORCEMENTS,
            Attribute.WAYPOINT_TRANSMIT_RANGE, Attribute.WAYPOINT_RECEIVE_RANGE,
            Attribute.AIR_DRAG_MODIFIER, Attribute.FRICTION_MODIFIER,
            Attribute.BOUNCINESS, Attribute.BELOW_NAME_DISTANCE,
            Attribute.NAME_TAG_DISTANCE
        };
        if (fields.length != 40 || Attribute.values().length != 40) {
            throw new AssertionError("Paper 26.2 exposes 40 canonical attributes");
        }
        for (Attribute field : fields) {
            if (field == null || Attribute.valueOf(field.name()) != field) {
                throw new AssertionError("attribute field/valueOf identity is not canonical");
            }
        }
    }
}
