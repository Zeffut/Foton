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
        String[] expectedOrder = {
            "AIR_DRAG_MODIFIER", "ARMOR", "ARMOR_TOUGHNESS", "ATTACK_DAMAGE",
            "ATTACK_KNOCKBACK", "ATTACK_SPEED", "BELOW_NAME_DISTANCE",
            "BLOCK_BREAK_SPEED", "BLOCK_INTERACTION_RANGE", "BOUNCINESS",
            "BURNING_TIME", "CAMERA_DISTANCE", "EXPLOSION_KNOCKBACK_RESISTANCE",
            "ENTITY_INTERACTION_RANGE", "FALL_DAMAGE_MULTIPLIER", "FLYING_SPEED",
            "FOLLOW_RANGE", "FRICTION_MODIFIER", "GRAVITY", "JUMP_STRENGTH",
            "KNOCKBACK_RESISTANCE", "LUCK", "MAX_ABSORPTION", "MAX_HEALTH",
            "MINING_EFFICIENCY", "MOVEMENT_EFFICIENCY", "MOVEMENT_SPEED",
            "NAME_TAG_DISTANCE", "OXYGEN_BONUS", "SAFE_FALL_DISTANCE", "SCALE",
            "SNEAKING_SPEED", "SPAWN_REINFORCEMENTS", "STEP_HEIGHT",
            "SUBMERGED_MINING_SPEED", "SWEEPING_DAMAGE_RATIO", "TEMPT_RANGE",
            "WATER_MOVEMENT_EFFICIENCY", "WAYPOINT_TRANSMIT_RANGE",
            "WAYPOINT_RECEIVE_RANGE"
        };
        for (Attribute field : fields) {
            if (field == null || Attribute.valueOf(field.name()) != field) {
                throw new AssertionError("attribute field/valueOf identity is not canonical");
            }
        }
        Attribute[] values = Attribute.values();
        for (int index = 0; index < values.length; index++) {
            if (!expectedOrder[index].equals(values[index].name())
                    || values[index].ordinal() != index) {
                throw new AssertionError("attribute order differs from the 26.2 asset at " + index);
            }
        }
        expectMetadata(Attribute.ARMOR, 0.0, "attribute.name.armor",
            Attribute.Sentiment.POSITIVE);
        expectMetadata(Attribute.BURNING_TIME, 1.0, "attribute.name.burning_time",
            Attribute.Sentiment.NEGATIVE);
        expectMetadata(Attribute.GRAVITY, 0.08, "attribute.name.gravity",
            Attribute.Sentiment.NEUTRAL);
        expectMetadata(Attribute.MAX_HEALTH, 20.0, "attribute.name.max_health",
            Attribute.Sentiment.POSITIVE);
        expectMetadata(Attribute.SCALE, 1.0, "attribute.name.scale",
            Attribute.Sentiment.NEUTRAL);
    }

    private static void expectMetadata(Attribute attribute, double defaultValue,
            String translationKey, Attribute.Sentiment sentiment) {
        if (Double.compare(attribute.getDefaultValue(), defaultValue) != 0
                || !translationKey.equals(attribute.getTranslationKey())
                || attribute.getSentiment() != sentiment) {
            throw new AssertionError("attribute metadata differs from the 26.2 asset for "
                + attribute.name());
        }
    }
}
