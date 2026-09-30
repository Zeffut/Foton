/** Runs a Paper-compiled Attribute consumer with only Foton's API implementation. */
public final class FotonAttributeBinaryRunner {
    private FotonAttributeBinaryRunner() {}

    public static void main(String[] args) {
        PaperAttributeConsumer.invokeInterface(org.bukkit.attribute.Attribute.ARMOR);
        PaperAttributeOldEnumConsumer.invokeOldEnum(org.bukkit.attribute.Attribute.ARMOR);
    }
}
