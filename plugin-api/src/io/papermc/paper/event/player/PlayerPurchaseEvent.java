package io.papermc.paper.event.player;

import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.event.player.PlayerEvent;
import org.bukkit.inventory.Merchant;
import org.bukkit.inventory.MerchantRecipe;

/** A player is about to take a trade from a merchant's screen, once per
 * trade, shift-clicks included. Cancelling leaves the trade undone. */
public class PlayerPurchaseEvent extends PlayerEvent implements Cancellable {
    private static final HandlerList HANDLER_LIST = new HandlerList();
    private final Merchant merchant;
    private boolean rewardExp;
    private boolean increaseTradeUses;
    private MerchantRecipe trade;
    private boolean cancelled;

    public PlayerPurchaseEvent(Player player, Merchant merchant, MerchantRecipe trade, boolean rewardExp,
            boolean increaseTradeUses) {
        super(player);
        this.merchant = merchant;
        this.trade = trade;
        this.rewardExp = rewardExp;
        this.increaseTradeUses = increaseTradeUses;
    }

    public Merchant getMerchant() { return merchant; }
    public MerchantRecipe getTrade() { return trade; }
    public void setTrade(MerchantRecipe trade) {
        if (trade == null) throw new IllegalArgumentException("Trade cannot be null!");
        this.trade = trade;
    }
    public boolean isRewardingExp() { return rewardExp; }
    public void setRewardExp(boolean rewardExp) { this.rewardExp = rewardExp; }
    public boolean willIncreaseTradeUses() { return increaseTradeUses; }
    public void setIncreaseTradeUses(boolean increaseTradeUses) { this.increaseTradeUses = increaseTradeUses; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancel) { cancelled = cancel; }
    @Override public HandlerList getHandlers() { return HANDLER_LIST; }
    public static HandlerList getHandlerList() { return HANDLER_LIST; }
}
