package org.bukkit.block;

import com.destroystokyo.paper.profile.PlayerProfile;

/** Skull block state profile API. */
public interface Skull extends BlockState {
    org.bukkit.OfflinePlayer getOwningPlayer();
    void setOwningPlayer(org.bukkit.OfflinePlayer player);
    PlayerProfile getPlayerProfile();
    void setPlayerProfile(PlayerProfile profile);
    io.papermc.paper.datacomponent.item.ResolvableProfile getProfile();
    void setProfile(io.papermc.paper.datacomponent.item.ResolvableProfile profile);
}
