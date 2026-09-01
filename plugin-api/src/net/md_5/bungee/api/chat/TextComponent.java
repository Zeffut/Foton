package net.md_5.bungee.api.chat;
public class TextComponent extends BaseComponent {
    public TextComponent() {}
    public TextComponent(String text) { this.text = text == null ? "" : text; }
    public TextComponent(BaseComponent... components) { if (components != null) for (BaseComponent c : components) addExtra(c); }
    public void setText(String text) { this.text = text == null ? "" : text; }
    public void addExtra(String text) { addExtra(new TextComponent(text)); }
    public static BaseComponent[] fromLegacyText(String text) { return new BaseComponent[] { new TextComponent(text) }; }
}
