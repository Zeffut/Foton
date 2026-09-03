package io.papermc.paper.command.brigadier;

/** Minimal Paper serializer handle used by Brigadier message adapters. */
public final class MessageComponentSerializer {
    private static final MessageComponentSerializer INSTANCE = new MessageComponentSerializer();
    private MessageComponentSerializer() { }
    public static MessageComponentSerializer message() { return INSTANCE; }
    public String serialize(net.kyori.adventure.text.Component component) {
        return component == null ? "" : net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer.plainText().serialize(component);
    }
}
