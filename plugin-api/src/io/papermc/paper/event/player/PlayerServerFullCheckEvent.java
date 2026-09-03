package io.papermc.paper.event.player;

import net.kyori.adventure.text.Component;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;
import com.destroystokyo.paper.profile.PlayerProfile;

/** Paper event fired while deciding whether a profile may join a full server. */
public final class PlayerServerFullCheckEvent extends Event {
    private final PlayerProfile playerProfile;
    private boolean allowed;
    private Component denialReason;
    private static final HandlerList HANDLERS = new HandlerList();

    public PlayerServerFullCheckEvent(PlayerProfile playerProfile) {
        this.playerProfile = playerProfile;
    }
    public PlayerProfile getPlayerProfile() { return playerProfile; }
    public boolean isAllowed() { return allowed; }
    public void allow() { allowed = true; denialReason = null; }
    public void allow(boolean value) { if (value) allow(); else deny(null); }
    public void deny(Component reason) { allowed = false; denialReason = reason; }
    public Component denialReason() { return denialReason; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
