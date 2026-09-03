package net.kyori.adventure.text.serializer.legacy;

import net.kyori.adventure.text.Component;

public final class LegacyComponentSerializer {
    private LegacyComponentSerializer() {}
    public static LegacyComponentSerializer legacySection() { return new LegacyComponentSerializer(); }
    public Component deserialize(String input) { return Component.text(input == null ? "" : input); }
    public String serialize(Component component) { return component == null ? "" : component.toString(); }
}
