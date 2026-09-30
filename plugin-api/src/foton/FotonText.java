package foton;

import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.serializer.gson.GsonComponentSerializer;
import net.kyori.adventure.text.serializer.json.JSONOptions;

/** Components cross to and from Rust as Minecraft's JSON text, which is what
 * Adventure's Gson serializer reads and writes. */
final class FotonText {
    /** Writes every component as an object. Adventure's default shortens a
     * bare text child to a JSON string, which the client reads but Foton's
     * component model, derived field by field, does not. */
    private static final GsonComponentSerializer SERIALIZER = GsonComponentSerializer.builder()
        .editOptions(options -> options.value(JSONOptions.EMIT_COMPACT_TEXT_COMPONENT, false))
        .build();

    private FotonText() {}

    /** The JSON text of a component, or null for none. */
    static String json(Component component) {
        return component == null ? null : SERIALIZER.serialize(component);
    }

    /** The component a JSON text describes; text that is not JSON is taken
     * as a plain string. Null stays null. */
    static Component component(String json) {
        if (json == null) return null;
        try {
            return SERIALIZER.deserialize(json);
        } catch (RuntimeException notJson) {
            return Component.text(json);
        }
    }
}
