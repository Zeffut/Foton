package org.bukkit.entity;

/** Vanilla iron golem entity view. */
public interface IronGolem extends Golem {
    boolean isPlayerCreated();
    void setPlayerCreated(boolean playerCreated);
}
