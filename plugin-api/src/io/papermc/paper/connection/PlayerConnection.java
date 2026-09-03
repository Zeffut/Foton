package io.papermc.paper.connection;

/** Connection context exposed during login validation. */
public interface PlayerConnection {
    default String getRemoteAddress() { return ""; }
}
