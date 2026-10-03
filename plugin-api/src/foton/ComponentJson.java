package foton;

import net.kyori.adventure.text.Component;
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
}
