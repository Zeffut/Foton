package io.papermc.paper.event.player;

import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.event.player.PlayerEvent;
import org.bukkit.inventory.MerchantRecipe;

/** Paper event fired when a player completes a merchant trade. */
public class PlayerTradeEvent extends PlayerEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final MerchantRecipe trade;
    private boolean cancelled;

    public PlayerTradeEvent(Player player, MerchantRecipe trade) {
        super(player);
        this.trade = trade;
    }
    public MerchantRecipe getTrade() { return trade; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
