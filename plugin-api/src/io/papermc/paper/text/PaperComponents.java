package io.papermc.paper.text;

/** Paper component serializer accessors used by modern plugins. */
public final class PaperComponents {
    private PaperComponents() {}

    /** Returns Paper's shared plain-text Adventure serializer. */
    public static net.kyori.adventure.text.serializer.plain.PlainComponentSerializer plainSerializer() {
        return net.kyori.adventure.text.serializer.plain.PlainComponentSerializer.plain();
    }

    public static net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer plainTextSerializer() {
        return net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer.plainText();
    }

    public static net.kyori.adventure.text.serializer.legacy.LegacyComponentSerializer legacySectionSerializer() {
        return net.kyori.adventure.text.serializer.legacy.LegacyComponentSerializer.legacySection();
    }
}
