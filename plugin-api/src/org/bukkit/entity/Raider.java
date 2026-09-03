package org.bukkit.entity;

/** Mob that participates in a vanilla patrol or raid. */
public interface Raider extends Monster {
    boolean isPatrolLeader();
    void setPatrolLeader(boolean leader);
}
