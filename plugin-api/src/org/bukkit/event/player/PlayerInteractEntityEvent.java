package org.bukkit.event.player;

import org.bukkit.entity.Entity;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.inventory.EquipmentSlot;

/** A player right-clicked an entity, with one hand: the client asks once per
 * hand, and a plugin acting once per click keeps only {@code HAND}. */
public class PlayerInteractEntityEvent extends PlayerEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    protected Entity clickedEntity;
    private final EquipmentSlot hand;
    private boolean cancelled;

    public PlayerInteractEntityEvent(Player player, Entity rightClicked) {
        this(player, rightClicked, EquipmentSlot.HAND);
    }
    public PlayerInteractEntityEvent(Player player, Entity rightClicked, EquipmentSlot hand) {
        super(player);
        this.clickedEntity = rightClicked;
        this.hand = hand;
    }

    public Entity getRightClicked() { return clickedEntity; }
    public EquipmentSlot getHand() { return hand; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
