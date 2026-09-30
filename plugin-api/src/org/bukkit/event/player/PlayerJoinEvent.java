package org.bukkit.event.player;

import net.kyori.adventure.text.Component;
import org.bukkit.entity.Player;
import org.bukkit.event.HandlerList;

/** A player joined; what is announced is {@link #joinMessage()}, and null
 * announces nothing. */
public class PlayerJoinEvent extends PlayerEvent {
    private static final HandlerList HANDLERS = new HandlerList();
    private Component joinMessage;

    public PlayerJoinEvent(Player playerJoined, Component joinMessage) {
        super(playerJoined);
        this.joinMessage = joinMessage;
    }

    @Deprecated
    public PlayerJoinEvent(Player playerJoined, String joinMessage) {
        this(playerJoined, fromLegacy(joinMessage));
    }

    /** A legacy string becomes a text component holding it: the client draws
     * its section-sign colour codes itself. */
    static Component fromLegacy(String text) { return text == null ? null : Component.text(text); }

    /** The text a legacy getter answers: a component that is only a string
     * gives that string back, anything richer its plain text. */
    static String toLegacy(Component component) {
        if (component == null) return null;
        if (component instanceof net.kyori.adventure.text.TextComponent text && text.children().isEmpty()
                && text.style().isEmpty()) {
            return text.content();
        }
        return net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer.plainText().serialize(component);
    }

    public Component joinMessage() { return joinMessage; }
    public void joinMessage(Component joinMessage) { this.joinMessage = joinMessage; }
    @Deprecated public String getJoinMessage() { return toLegacy(joinMessage); }
    @Deprecated public void setJoinMessage(String joinMessage) { this.joinMessage = fromLegacy(joinMessage); }

    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
