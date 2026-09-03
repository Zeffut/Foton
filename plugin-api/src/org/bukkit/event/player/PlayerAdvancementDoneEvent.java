package org.bukkit.event.player;
import org.bukkit.advancement.Advancement;
import org.bukkit.entity.Player;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;
public class PlayerAdvancementDoneEvent extends Event {
    private final Player player; private final Advancement advancement;
    private net.kyori.adventure.text.Component message;
    private static final HandlerList HANDLERS = new HandlerList();
    public PlayerAdvancementDoneEvent(Player player, Advancement advancement) { this.player = player; this.advancement = advancement; }
    public Player getPlayer() { return player; }
    public Advancement getAdvancement() { return advancement; }
    public net.kyori.adventure.text.Component message() { return message; }
    public void message(net.kyori.adventure.text.Component value) { message = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
