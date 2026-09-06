package io.papermc.paper.command.brigadier;

/** Minimal Paper serializer handle used by Brigadier message adapters. */
public final class MessageComponentSerializer {
    private static final MessageComponentSerializer INSTANCE = new MessageComponentSerializer();
    private MessageComponentSerializer() { }
    public static MessageComponentSerializer message() { return INSTANCE; }
    /** Reads a Brigadier message back as a component, or null when there is
     * nothing to read.
     *
     * <p>Takes {@code Object} because that is what Paper's signature takes:
     * the argument is a Brigadier {@code Message}, and accepting the interface
     * loosely keeps this compiling whether or not Brigadier is on the path at
     * the call site. Plain text on the way back -- a Brigadier message carries
     * no styling to recover. */
    public net.kyori.adventure.text.Component deserializeOrNull(Object message) {
        if (message == null) return null;
        String text = message instanceof com.mojang.brigadier.Message brigadier
                ? brigadier.getString()
                : message.toString();
        return text == null ? null : net.kyori.adventure.text.Component.text(text);
    }

    public net.kyori.adventure.text.Component deserialize(Object message) {
        net.kyori.adventure.text.Component read = deserializeOrNull(message);
        return read == null ? net.kyori.adventure.text.Component.empty() : read;
    }

    public String serialize(net.kyori.adventure.text.Component component) {
        return component == null ? "" : net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer.plainText().serialize(component);
    }
}
