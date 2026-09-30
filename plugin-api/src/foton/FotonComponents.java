package foton;

import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.KeybindComponent;
import net.kyori.adventure.text.TextComponent;
import net.kyori.adventure.text.TranslatableComponent;
import net.kyori.adventure.text.TranslationArgument;
import net.kyori.adventure.text.event.ClickEvent;
import net.kyori.adventure.text.event.HoverEvent;
import net.kyori.adventure.text.format.NamedTextColor;
import net.kyori.adventure.text.format.Style;
import net.kyori.adventure.text.format.TextColor;
import net.kyori.adventure.text.format.TextDecoration;

/** Adventure components as vanilla's JSON text format, for Foton to decode.
 *
 * <p>A component crosses whole -- colour, decorations, translations and
 * children -- because a name tag or a text display that loses its colour on
 * the way is not the thing the plugin made. Content vanilla resolves on the
 * server (scores, selectors, NBT) crosses as its plain text. */
public final class FotonComponents {
    private FotonComponents() { }

    /** The component as vanilla JSON, or null for null. */
    public static String toJson(Component component) {
        if (component == null) return null;
        StringBuilder out = new StringBuilder();
        write(out, component);
        return out.toString();
    }

    private static void write(StringBuilder out, Component component) {
        out.append('{');
        if (component instanceof TextComponent text) {
            field(out, "text").append(quote(text.content()));
        } else if (component instanceof TranslatableComponent translatable) {
            field(out, "translate").append(quote(translatable.key()));
            if (translatable.fallback() != null) field(out, "fallback").append(quote(translatable.fallback()));
            if (!translatable.arguments().isEmpty()) {
                field(out, "with").append('[');
                boolean first = true;
                for (TranslationArgument argument : translatable.arguments()) {
                    if (!first) out.append(',');
                    first = false;
                    if (argument.value() instanceof Component nested) write(out, nested);
                    else out.append("{\"text\":").append(quote(String.valueOf(argument.value()))).append('}');
                }
                out.append(']');
            }
        } else if (component instanceof KeybindComponent keybind) {
            field(out, "keybind").append(quote(keybind.keybind()));
        } else {
            field(out, "text").append(quote(
                net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer.plainText()
                    .serialize(component.children(java.util.List.of()))));
        }
        style(out, component.style());
        if (!component.children().isEmpty()) {
            field(out, "extra").append('[');
            boolean first = true;
            for (Component child : component.children()) {
                if (!first) out.append(',');
                first = false;
                write(out, child);
            }
            out.append(']');
        }
        out.append('}');
    }

    private static void style(StringBuilder out, Style style) {
        TextColor color = style.color();
        if (color != null) {
            String named = color instanceof NamedTextColor value ? NamedTextColor.NAMES.key(value) : null;
            field(out, "color").append(quote(named != null ? named : color.asHexString()));
        }
        decoration(out, style, TextDecoration.BOLD, "bold");
        decoration(out, style, TextDecoration.ITALIC, "italic");
        decoration(out, style, TextDecoration.UNDERLINED, "underlined");
        decoration(out, style, TextDecoration.STRIKETHROUGH, "strikethrough");
        decoration(out, style, TextDecoration.OBFUSCATED, "obfuscated");
        if (style.font() != null) field(out, "font").append(quote(style.font().asString()));
        if (style.insertion() != null) field(out, "insertion").append(quote(style.insertion()));
        ClickEvent<?> click = style.clickEvent();
        if (click != null) {
            String action = ClickEvent.Action.NAMES.key(click.action());
            String member = switch (action == null ? "" : action) {
                case "open_url" -> "url";
                case "run_command", "suggest_command" -> "command";
                case "change_page" -> "page";
                case "copy_to_clipboard" -> "value";
                default -> null;
            };
            if (member != null) {
                field(out, "click_event").append("{\"action\":").append(quote(action)).append(',')
                    .append(quote(member)).append(':');
                if (click.payload() instanceof ClickEvent.Payload.Int page) out.append(page.integer());
                else if (click.payload() instanceof ClickEvent.Payload.Text text) out.append(quote(text.value()));
                else out.append("\"\"");
                out.append('}');
            }
        }
        HoverEvent<?> hover = style.hoverEvent();
        if (hover != null && hover.value() instanceof Component text) {
            field(out, "hover_event").append("{\"action\":\"show_text\",\"value\":");
            write(out, text);
            out.append('}');
        }
    }

    private static void decoration(StringBuilder out, Style style, TextDecoration decoration, String name) {
        TextDecoration.State state = style.decoration(decoration);
        if (state == TextDecoration.State.TRUE) field(out, name).append("true");
        else if (state == TextDecoration.State.FALSE) field(out, name).append("false");
    }

    private static StringBuilder field(StringBuilder out, String name) {
        if (out.charAt(out.length() - 1) != '{') out.append(',');
        return out.append('"').append(name).append("\":");
    }

    private static String quote(String value) {
        StringBuilder out = new StringBuilder(value.length() + 2).append('"');
        for (int index = 0; index < value.length(); index++) {
            char c = value.charAt(index);
            switch (c) {
                case '"' -> out.append("\\\"");
                case '\\' -> out.append("\\\\");
                case '\n' -> out.append("\\n");
                case '\r' -> out.append("\\r");
                case '\t' -> out.append("\\t");
                default -> {
                    if (c < 0x20) out.append(String.format("\\u%04x", (int) c));
                    else out.append(c);
                }
            }
        }
        return out.append('"').toString();
    }
}
