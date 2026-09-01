package net.md_5.bungee.api;
public final class ChatColor {
    public static final ChatColor DARK_AQUA = new ChatColor("§3");
    public static final ChatColor GRAY = new ChatColor("§7");
    public static final ChatColor GREEN = new ChatColor("§a");
    public static final ChatColor LIGHT_PURPLE = new ChatColor("§d");
    public static final ChatColor RED = new ChatColor("§c");
    public static final ChatColor WHITE = new ChatColor("§f");
    private final String code;
    private ChatColor(String code) { this.code = code; }
    public static ChatColor of(String value) { return new ChatColor(value); }
    public static String translateAlternateColorCodes(char alt, String text) { return text == null ? null : text.replace(alt, '§'); }
    @Override public String toString() { return code; }
}
