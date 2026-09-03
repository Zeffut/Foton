package net.md_5.bungee.api.chat;
import net.md_5.bungee.api.ChatColor;
public class ComponentBuilder {
    private final TextComponent component;
    public ComponentBuilder(String text) { component = new TextComponent(text); }
    public ComponentBuilder color(ChatColor color) { component.setColor(color); return this; }
    public BaseComponent[] create() { return new BaseComponent[] { component }; }
}
