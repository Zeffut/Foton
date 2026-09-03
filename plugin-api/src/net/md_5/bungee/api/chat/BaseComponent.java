package net.md_5.bungee.api.chat;
import net.md_5.bungee.api.ChatColor;
public class BaseComponent {
    protected String text = "";
    private final java.util.List<BaseComponent> extra = new java.util.ArrayList<>();
    private ChatColor color;
    private Boolean bold, italic, underlined, strikethrough, obfuscated;
    private String font;
    public BaseComponent() {}
    protected BaseComponent(BaseComponent... components) {
        if (components != null) for (BaseComponent component : components)
            if (component != null) extra.add(component);
    }
    public final void addExtra(BaseComponent component) { if (component != null) extra.add(component); }
    public void setColor(ChatColor value) { color = value; }
    public void setBold(Boolean value) { bold = value; }
    public void setItalic(Boolean value) { italic = value; }
    public void setUnderlined(Boolean value) { underlined = value; }
    public void setStrikethrough(Boolean value) { strikethrough = value; }
    public void setObfuscated(Boolean value) { obfuscated = value; }
    public void setFont(String value) { font = value; }
    public String toPlainText() { StringBuilder result = new StringBuilder(text); for (BaseComponent child : extra) result.append(child.toPlainText()); return result.toString(); }
    public String toLegacyText() {
        StringBuilder prefix = new StringBuilder(color == null ? "" : color.toString());
        if (Boolean.TRUE.equals(bold)) prefix.append("§l");
        if (Boolean.TRUE.equals(italic)) prefix.append("§o");
        if (Boolean.TRUE.equals(underlined)) prefix.append("§n");
        if (Boolean.TRUE.equals(strikethrough)) prefix.append("§m");
        if (Boolean.TRUE.equals(obfuscated)) prefix.append("§k");
        StringBuilder result = new StringBuilder(prefix).append(text);
        for (BaseComponent child : extra) result.append(child.toLegacyText());
        return result.toString();
    }
}
