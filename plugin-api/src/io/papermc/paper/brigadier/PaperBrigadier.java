package io.papermc.paper.brigadier;

import com.mojang.brigadier.Message;
import net.kyori.adventure.text.Component;

/** Bridge between Brigadier messages and Adventure components.
 *
 * <p>Paper's own class carries conversions in both directions, including ones
 * that name server internals. Only the direction whose signature stays inside
 * public types is here -- a Brigadier {@code Message} in, a {@code Component}
 * out. The internals-facing half cannot exist on a server that is not Mojang's.
 */
public final class PaperBrigadier {
    private PaperBrigadier() { }

    /** Reads a Brigadier message as a component.
     *
     * <p>Plain text, because a Brigadier message carries no styling to recover
     * -- it is a string with a lazily computed source, not a rich document. */
    public static Component componentFromMessage(Message message) {
        return message == null ? Component.empty() : Component.text(message.getString());
    }

    /** Wraps a component as a Brigadier message, keeping its plain text. */
    public static Message message(Component component) {
        String text = component == null
                ? ""
                : net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer
                        .plainText().serialize(component);
        return () -> text;
    }
}
