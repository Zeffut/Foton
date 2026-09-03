package org.bukkit.event.player;

import org.bukkit.Statistic;
import org.bukkit.entity.Player;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired when a player statistic changes. */
public final class PlayerStatisticIncrementEvent extends Event {
    private final Player player;
    private final Statistic statistic;
    private static final HandlerList HANDLERS = new HandlerList();
    public PlayerStatisticIncrementEvent(Player player, Statistic statistic) { this.player = player; this.statistic = statistic; }
    public Player getPlayer() { return player; }
    public Statistic getStatistic() { return statistic; }
    public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
