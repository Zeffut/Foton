package org.bukkit.entity;

/** Vanilla phantom entity view. */
public interface Phantom extends Flying, Monster {
    int getSize();
    void setSize(int size);
}
