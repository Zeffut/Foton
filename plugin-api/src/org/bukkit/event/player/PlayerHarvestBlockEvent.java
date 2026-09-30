package org.bukkit.event.player;

import java.util.List;
import org.bukkit.block.Block;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.inventory.EquipmentSlot;
import org.bukkit.inventory.ItemStack;

/** A player picks from a block without breaking it: sweet berries, glow
 * berries. The stacks may be changed; cancelling leaves the block unpicked. */
public class PlayerHarvestBlockEvent extends PlayerEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final Block harvestedBlock;
    private final EquipmentSlot hand;
    private final List<ItemStack> itemsHarvested;
    private boolean cancelled;

    public PlayerHarvestBlockEvent(Player player, Block harvestedBlock, EquipmentSlot hand,
            List<ItemStack> itemsHarvested) {
        super(player);
        this.harvestedBlock = harvestedBlock;
        this.hand = hand;
        this.itemsHarvested = itemsHarvested;
    }

    public Block getHarvestedBlock() { return harvestedBlock; }
    public EquipmentSlot getHand() { return hand; }
    public List<ItemStack> getItemsHarvested() { return itemsHarvested; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancel) { cancelled = cancel; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
