import org.bukkit.attribute.Attribute;

/** Exercises Paper's erased OldEnum compareTo descriptor separately. */
public final class PaperAttributeOldEnumConsumer {
    private PaperAttributeOldEnumConsumer() {}

    public static void invokeOldEnum(Attribute attribute) {
        if (attribute.compareTo(attribute) != 0) {
            throw new AssertionError("an attribute must compare equal to itself");
        }
    }
}
