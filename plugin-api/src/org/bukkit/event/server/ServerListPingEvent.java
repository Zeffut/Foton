package org.bukkit.event.server;

import java.net.InetAddress;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Iterator;
import java.util.List;
import org.bukkit.entity.Player;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

public class ServerListPingEvent extends Event implements Iterable<Player> {
    private static final HandlerList HANDLERS = new HandlerList();
    private final InetAddress address;
    private final List<Player> players;
    private String motd;
    private int maxPlayers;
    public ServerListPingEvent(InetAddress address, String motd, int numPlayers, int maxPlayers) {
        this(address, motd, Collections.emptyList(), maxPlayers);
    }
    public ServerListPingEvent(InetAddress address, String motd, List<Player> players, int maxPlayers) {
        this.address = address; this.motd = motd == null ? "" : motd;
        this.players = new ArrayList<>(players == null ? Collections.emptyList() : players);
        this.maxPlayers = maxPlayers;
    }
    public InetAddress getAddress() { return address; }
    public String getMotd() { return motd; }
    public void setMotd(String value) { motd = value == null ? "" : value; }
    public int getNumPlayers() { return players.size(); }
    public int getMaxPlayers() { return maxPlayers; }
    public void setMaxPlayers(int value) { maxPlayers = value; }
    @Override public Iterator<Player> iterator() { return Collections.unmodifiableList(players).iterator(); }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
