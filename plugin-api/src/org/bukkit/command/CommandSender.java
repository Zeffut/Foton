package org.bukkit.command;

public interface CommandSender {
    void sendMessage(String message);
    /**
     * Adventure-compatible overload. Steel's command transport currently
     * accepts plain text, so the component is reduced using Adventure's
     * canonical plain-text serializer before delivery.
     */
    default void sendMessage(net.kyori.adventure.text.Component component) {
        if (component != null) {
            sendMessage(net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer
                    .plainText().serialize(component));
        }
    }
    default void sendMessage(net.md_5.bungee.api.chat.BaseComponent component) {
        if (component != null) sendMessage(component.toLegacyText());
    }
    default void sendMessage(net.md_5.bungee.api.chat.BaseComponent... components) {
        if (components != null) spigot().sendMessage(components);
    }
    boolean hasPermission(String permission);
    default boolean isOp() { return false; }
    default boolean isPermissionSet(String permission) { return false; }
    String getName();
    default Spigot spigot() { return new Spigot(this); }
    class Spigot {
        private final CommandSender sender;
        public Spigot(CommandSender sender) { this.sender = sender; }
        public void sendMessage(net.md_5.bungee.api.chat.BaseComponent component) {
            if (component != null) sender.sendMessage(component.toLegacyText());
        }
        public void sendMessage(net.md_5.bungee.api.chat.BaseComponent... components) {
            if (components == null) return;
            StringBuilder text = new StringBuilder();
            for (net.md_5.bungee.api.chat.BaseComponent component : components)
                if (component != null) text.append(component.toLegacyText());
            sender.sendMessage(text.toString());
        }
    }
}
