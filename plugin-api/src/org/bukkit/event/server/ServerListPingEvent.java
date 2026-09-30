package org.bukkit.event.server;

import java.net.InetAddress;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Iterator;
import java.util.List;
import net.kyori.adventure.text.Component;
import org.bukkit.entity.Player;
import org.bukkit.event.HandlerList;

/** A client asked for the server-list entry. The MOTD and the player limit
 * left here are what it is shown. Fired off the main thread, as in Paper. */
public class ServerListPingEvent extends ServerEvent implements Iterable<Player> {
    private static final HandlerList HANDLERS = new HandlerList();
    private final String hostname;
    private final InetAddress address;
    private final List<Player> players;
    private final int numPlayers;
    private Component motd;
    private int maxPlayers;

    public ServerListPingEvent(String hostname, InetAddress address, Component motd, int numPlayers, int maxPlayers) {
        super(true);
        this.hostname = hostname;
        this.address = address;
        this.motd = motd == null ? Component.empty() : motd;
        this.players = Collections.emptyList();
        this.numPlayers = numPlayers;
        this.maxPlayers = maxPlayers;
    }
    public ServerListPingEvent(InetAddress address, Component motd, int numPlayers, int maxPlayers) {
        this("", address, motd, numPlayers, maxPlayers);
    }
    @Deprecated
    public ServerListPingEvent(String hostname, InetAddress address, String motd, int numPlayers, int maxPlayers) {
        this(hostname, address, fromLegacy(motd == null ? "" : motd), numPlayers, maxPlayers);
    }
    @Deprecated
    public ServerListPingEvent(InetAddress address, String motd, int numPlayers, int maxPlayers) {
        this("", address, motd, numPlayers, maxPlayers);
    }
    public ServerListPingEvent(InetAddress address, String motd, List<Player> players, int maxPlayers) {
        super(true);
        this.hostname = "";
        this.address = address;
        this.motd = fromLegacy(motd == null ? "" : motd);
        this.players = new ArrayList<>(players == null ? Collections.emptyList() : players);
        this.numPlayers = this.players.size();
        this.maxPlayers = maxPlayers;
    }

    /** A legacy string becomes a text component holding it: the client draws
     * its section-sign colour codes itself. */
    private static Component fromLegacy(String text) { return text == null ? null : Component.text(text); }

    /** The text a legacy getter answers: a component that is only a string
     * gives that string back, anything richer its plain text. */
    private static String toLegacy(Component component) {
        if (component == null) return null;
        if (component instanceof net.kyori.adventure.text.TextComponent text && text.children().isEmpty()
                && text.style().isEmpty()) {
            return text.content();
        }
        return net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer.plainText().serialize(component);
    }

    public String getHostname() { return hostname; }
    public InetAddress getAddress() { return address; }
    public Component motd() { return motd; }
    public void motd(Component motd) { this.motd = motd == null ? Component.empty() : motd; }
    @Deprecated public String getMotd() { return toLegacy(motd); }
    @Deprecated public void setMotd(String motd) { this.motd = fromLegacy(motd == null ? "" : motd); }
    public int getNumPlayers() { return numPlayers; }
    public int getMaxPlayers() { return maxPlayers; }
    public void setMaxPlayers(int maxPlayers) { this.maxPlayers = maxPlayers; }
    @Override public Iterator<Player> iterator() { return Collections.unmodifiableList(players).iterator(); }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
