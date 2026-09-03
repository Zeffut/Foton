package net.kyori.adventure.text.serializer.plain;

import net.kyori.adventure.text.Component;

public final class PlainComponentSerializer {
    private static final PlainComponentSerializer INSTANCE = new PlainComponentSerializer();
    private PlainComponentSerializer() {}
    public static PlainComponentSerializer plain() { return INSTANCE; }
    public String serialize(Component component) { return component == null ? "" : component.toString(); }
    public Component deserialize(String input) { return Component.text(input == null ? "" : input); }
}
