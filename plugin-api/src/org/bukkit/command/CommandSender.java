package org.bukkit.command;

public interface CommandSender {
    void sendMessage(String message);
    default void sendMessage(net.md_5.bungee.api.chat.BaseComponent component) {
        if (component != null) sendMessage(component.toLegacyText());
    }
    default void sendMessage(net.md_5.bungee.api.chat.BaseComponent... components) {
        if (components != null) spigot().sendMessage(components);
    }
    boolean hasPermission(String permission);
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
