package net.kyori.adventure.text.serializer.plain;

import net.kyori.adventure.text.Component;

/** Adventure 4's name for the plain serializer, kept for plugins built against it. */
public final class PlainComponentSerializer {
    private static final PlainComponentSerializer INSTANCE = new PlainComponentSerializer();
    private PlainComponentSerializer() {}
    public static PlainComponentSerializer plain() { return INSTANCE; }
    public String serialize(Component component) { return component == null ? "" : net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer.plainText().serialize(component); }
    public Component deserialize(String input) { return Component.text(input == null ? "" : input); }
}
