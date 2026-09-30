package io.papermc.paper.event.player;

import org.bukkit.entity.AbstractVillager;
import org.bukkit.entity.Player;
import org.bukkit.inventory.MerchantRecipe;

/** A {@link PlayerPurchaseEvent} whose merchant is a villager or a wandering
 * trader. It shares its parent's handler list, as in Paper. */
public class PlayerTradeEvent extends PlayerPurchaseEvent {
    public PlayerTradeEvent(Player player, AbstractVillager villager, MerchantRecipe trade, boolean rewardExp,
            boolean increaseTradeUses) {
        super(player, villager, trade, rewardExp, increaseTradeUses);
    }

    @Override public AbstractVillager getMerchant() { return (AbstractVillager) super.getMerchant(); }
    public AbstractVillager getVillager() { return getMerchant(); }
}
