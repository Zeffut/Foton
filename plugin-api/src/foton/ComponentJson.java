package foton;

import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.TextComponent;
import net.kyori.adventure.text.TranslatableComponent;
import net.kyori.adventure.text.KeybindComponent;
import net.kyori.adventure.text.format.NamedTextColor;
import net.kyori.adventure.text.format.Style;
import net.kyori.adventure.text.format.TextDecoration;
import net.kyori.adventure.text.serializer.gson.GsonComponentSerializer;
import net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer;

/** Adventure's JSON serialization at the Java/native item boundary. */
public final class ComponentJson {
    private ComponentJson() { }

    public static String json(Component value) {
        return GsonComponentSerializer.gson().serialize(value);
    }

    public static Component parse(String value) {
        return GsonComponentSerializer.gson().deserialize(value);
    }

    public static String plain(Component value) {
        return PlainTextComponentSerializer.plainText().serialize(value);
    }

    private static final String CODES = "0123456789abcdef";
    private static final NamedTextColor[] ORDER = {
        NamedTextColor.BLACK, NamedTextColor.DARK_BLUE, NamedTextColor.DARK_GREEN, NamedTextColor.DARK_AQUA,
        NamedTextColor.DARK_RED, NamedTextColor.DARK_PURPLE, NamedTextColor.GOLD, NamedTextColor.GRAY,
        NamedTextColor.DARK_GRAY, NamedTextColor.BLUE, NamedTextColor.GREEN, NamedTextColor.AQUA,
        NamedTextColor.RED, NamedTextColor.LIGHT_PURPLE, NamedTextColor.YELLOW, NamedTextColor.WHITE,
    };

    /** Section-sign text, as Adventure's legacySection serializer writes it: a
     * hex color falls to the nearest named one, and a translation to its key. */
    public static String legacy(Component component) {
        StringBuilder out = new StringBuilder();
        legacy(component, Style.empty(), new String[] {""}, out);
        return out.toString();
    }

    private static void legacy(Component component, Style inherited, String[] emitted, StringBuilder out) {
        // The child's own style, with what it leaves unset taken from its parent.
        Style style = component.style().merge(inherited, Style.Merge.Strategy.IF_ABSENT_ON_TARGET);
        String content = component instanceof TextComponent text ? text.content()
            : component instanceof TranslatableComponent translatable ? translatable.key()
            : component instanceof KeybindComponent keybind ? keybind.keybind() : "";
        if (!content.isEmpty()) {
            String codes = codes(style);
            if (!codes.equals(emitted[0])) {
                // Legacy text cannot turn a format off, only reset everything.
                if (!emitted[0].isEmpty() && style.color() == null) out.append('§').append('r');
                out.append(codes);
                emitted[0] = codes;
            }
            out.append(content);
        }
        for (Component child : component.children()) legacy(child, style, emitted, out);
    }

    private static String codes(Style style) {
        StringBuilder codes = new StringBuilder();
        if (style.color() != null) {
            NamedTextColor named = NamedTextColor.nearestTo(style.color());
            for (int index = 0; index < ORDER.length; index++) {
                if (ORDER[index] == named) codes.append('§').append(CODES.charAt(index));
            }
        }
        if (style.decoration(TextDecoration.OBFUSCATED) == TextDecoration.State.TRUE) codes.append("§k");
        if (style.decoration(TextDecoration.BOLD) == TextDecoration.State.TRUE) codes.append("§l");
        if (style.decoration(TextDecoration.STRIKETHROUGH) == TextDecoration.State.TRUE) codes.append("§m");
        if (style.decoration(TextDecoration.UNDERLINED) == TextDecoration.State.TRUE) codes.append("§n");
        if (style.decoration(TextDecoration.ITALIC) == TextDecoration.State.TRUE) codes.append("§o");
        return codes.toString();
    }
}
