package org.bukkit.profile;

import java.util.UUID;
import java.util.concurrent.CompletableFuture;

public interface PlayerProfile {
    UUID getUniqueId();
    String getName();
    PlayerTextures getTextures();
    void setTextures(PlayerTextures textures);
    boolean isComplete();
    default CompletableFuture<PlayerProfile> update() { return CompletableFuture.completedFuture(this); }
}
