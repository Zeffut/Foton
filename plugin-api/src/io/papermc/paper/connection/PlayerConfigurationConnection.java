package io.papermc.paper.connection;

public interface PlayerConfigurationConnection extends PlayerConnection {
    /** Returns a client option when the connection exposes it; unavailable options are null. */
    default <T> T getClientOption(com.destroystokyo.paper.ClientOption<T> option) { return null; }
    default com.destroystokyo.paper.profile.PlayerProfile getProfile() { return null; }
}
