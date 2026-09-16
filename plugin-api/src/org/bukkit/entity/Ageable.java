package org.bukkit.entity;

/** Bukkit age state exposed by ageable mobs. */
public interface Ageable extends Creature {
    int getAge();
    void setAge(int age);
    void setAgeLock(boolean lock);
    boolean getAgeLock();
    void setBaby();
    void setAdult();
    boolean isAdult();
    boolean canBreed();
    void setBreed(boolean breed);
}
