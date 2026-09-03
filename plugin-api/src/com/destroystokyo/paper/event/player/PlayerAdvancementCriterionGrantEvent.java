package com.destroystokyo.paper.event.player;

import org.bukkit.advancement.Advancement;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.event.player.PlayerEvent;

/** Fired immediately before a player is granted one advancement criterion. */
public class PlayerAdvancementCriterionGrantEvent extends PlayerEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final Advancement advancement;
    private final String criterion;
    private boolean cancelled;

    public PlayerAdvancementCriterionGrantEvent(Player player, Advancement advancement, String criterion) {
        super(player);
        this.advancement = advancement;
        this.criterion = criterion;
    }
    public Advancement getAdvancement() { return advancement; }
    public String getCriterion() { return criterion; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
