package org.bukkit.event.player;

import java.util.Locale;
import org.bukkit.entity.Player;
import org.bukkit.event.HandlerList;

/** Fired when the client changes its language preference. */
public class PlayerLocaleChangeEvent extends PlayerEvent {
    private final String oldLocale;
    private final String locale;
    private static final HandlerList HANDLERS = new HandlerList();
    public PlayerLocaleChangeEvent(Player player, String oldLocale, String locale) { super(player); this.oldLocale=oldLocale; this.locale=locale; }
    public String getOldLocale() { return oldLocale; }
    public String getLocale() { return locale; }
    public Locale locale() { try { return Locale.forLanguageTag(locale.replace('_','-')); } catch (RuntimeException ignored) { return Locale.ROOT; } }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
