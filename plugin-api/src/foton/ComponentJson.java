package foton;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import java.util.Locale;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.KeybindComponent;
import net.kyori.adventure.text.TextComponent;
import net.kyori.adventure.text.TranslatableComponent;
import net.kyori.adventure.text.TranslationArgument;
import net.kyori.adventure.text.format.NamedTextColor;
import net.kyori.adventure.text.format.Style;
import net.kyori.adventure.text.format.TextColor;
import net.kyori.adventure.text.format.TextDecoration;

/** Adventure components in the two forms the server takes text in.
 *
 * <p>{@link #json} is vanilla's JSON text component, which is what reaches
 * the client wherever vanilla carries a component (book pages, team
 * prefixes); Foton parses it on the Rust side. {@link #legacy} is the
 * section-sign string vanilla still uses where it stores plain strings (a
 * book's title and author).</p>
 *
 * <p>Click and hover events are not carried: nothing that crosses here
 * today is clickable in vanilla, and a half-carried event would be worse
 * than none.</p>
 */
public final class ComponentJson {
    private ComponentJson() { }

    public static String json(Component component) {
        return tree(component).toString();
    }

    private static JsonObject tree(Component component) {
        JsonObject out = new JsonObject();
        if (component instanceof TextComponent text) {
            out.addProperty("text", text.content());
        } else if (component instanceof TranslatableComponent translatable) {
            out.addProperty("translate", translatable.key());
            if (translatable.fallback() != null) out.addProperty("fallback", translatable.fallback());
            if (!translatable.arguments().isEmpty()) {
                JsonArray with = new JsonArray();
                for (TranslationArgument argument : translatable.arguments()) {
                    Object value = argument.value();
                    if (value instanceof Component nested) with.add(tree(nested));
                    else if (value instanceof Boolean bool) with.add(bool);
                    else if (value instanceof Number number) with.add(number);
                    else with.add(String.valueOf(value));
                }
                out.add("with", with);
            }
        } else if (component instanceof KeybindComponent keybind) {
            out.addProperty("keybind", keybind.keybind());
        } else {
            // Selector, score and NBT components resolve against a sender the
            // server does not have here; their plain text is the honest rest.
            out.addProperty("text", net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer
                .plainText().serialize(component));
        }
        style(component.style(), out);
        if (!component.children().isEmpty()) {
            JsonArray extra = new JsonArray();
            for (Component child : component.children()) extra.add(tree(child));
            out.add("extra", extra);
        }
        return out;
    }

    /** Reads what {@link #json} writes, and vanilla's other shapes of it: a
     * bare string, or an array whose first element carries the rest. */
    public static Component parse(String json) {
        if (json == null || json.isEmpty()) return Component.empty();
        try {
            return read(com.google.gson.JsonParser.parseString(json));
        } catch (RuntimeException malformed) {
            return Component.text(json);
        }
    }

    private static Component read(com.google.gson.JsonElement element) {
        if (element.isJsonPrimitive()) return Component.text(element.getAsString());
        if (element.isJsonArray()) {
            JsonArray array = element.getAsJsonArray();
            if (array.isEmpty()) return Component.empty();
            Component first = read(array.get(0));
            for (int index = 1; index < array.size(); index++) first = first.append(read(array.get(index)));
            return first;
        }
        JsonObject object = element.getAsJsonObject();
        Component out;
        if (object.has("translate")) {
            java.util.List<Component> with = new java.util.ArrayList<>();
            if (object.has("with")) for (com.google.gson.JsonElement argument : object.getAsJsonArray("with")) with.add(read(argument));
            String fallback = object.has("fallback") ? object.get("fallback").getAsString() : null;
            out = Component.translatable(object.get("translate").getAsString(), fallback, with);
        } else if (object.has("keybind")) {
            out = Component.keybind(object.get("keybind").getAsString());
        } else {
            out = Component.text(object.has("text") ? object.get("text").getAsString() : "");
        }
        Style.Builder style = Style.style();
        if (object.has("color")) {
            String color = object.get("color").getAsString();
            TextColor value = color.startsWith("#") ? TextColor.fromHexString(color) : NamedTextColor.NAMES.value(color);
            if (value != null) style.color(value);
        }
        for (TextDecoration decoration : TextDecoration.values()) {
            String name = decoration.name().toLowerCase(Locale.ROOT);
            if (object.has(name)) style.decoration(decoration, object.get(name).getAsBoolean());
        }
        if (object.has("font")) style.font(net.kyori.adventure.key.Key.key(object.get("font").getAsString()));
        if (object.has("insertion")) style.insertion(object.get("insertion").getAsString());
        out = out.style(style.build());
        if (object.has("shadow_color")) {
            out = out.shadowColor(net.kyori.adventure.text.format.ShadowColor.shadowColor(object.get("shadow_color").getAsInt()));
        }
        if (object.has("extra")) for (com.google.gson.JsonElement child : object.getAsJsonArray("extra")) out = out.append(read(child));
        return out;
    }

    private static void style(Style style, JsonObject out) {
        TextColor color = style.color();
        if (color != null) out.addProperty("color", colorName(color));
        for (TextDecoration decoration : TextDecoration.values()) {
            TextDecoration.State state = style.decoration(decoration);
            if (state == TextDecoration.State.NOT_SET) continue;
            out.addProperty(decoration.name().toLowerCase(Locale.ROOT), state == TextDecoration.State.TRUE);
        }
        if (style.font() != null) out.addProperty("font", style.font().asString());
        if (style.shadowColor() != null) out.addProperty("shadow_color", style.shadowColor().value());
        if (style.insertion() != null) out.addProperty("insertion", style.insertion());
    }

    private static String colorName(TextColor color) {
        if (color instanceof NamedTextColor named) {
            String name = NamedTextColor.NAMES.key(named);
            if (name != null) return name;
        }
        return color.asHexString();
    }

    private static final String CODES = "0123456789abcdef";
    private static final NamedTextColor[] ORDER = {
        NamedTextColor.BLACK, NamedTextColor.DARK_BLUE, NamedTextColor.DARK_GREEN, NamedTextColor.DARK_AQUA,
        NamedTextColor.DARK_RED, NamedTextColor.DARK_PURPLE, NamedTextColor.GOLD, NamedTextColor.GRAY,
        NamedTextColor.DARK_GRAY, NamedTextColor.BLUE, NamedTextColor.GREEN, NamedTextColor.AQUA,
        NamedTextColor.RED, NamedTextColor.LIGHT_PURPLE, NamedTextColor.YELLOW, NamedTextColor.WHITE,
    };

    /** Section-sign text, as Adventure's legacySection serializer writes it: a
     * hex colour falls to the nearest named one, and a translation to its key. */
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
