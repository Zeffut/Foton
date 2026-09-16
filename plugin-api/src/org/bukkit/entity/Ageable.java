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
    @Deprecated(since = "1.16.2")
    boolean canBreed();
    @Deprecated(since = "1.16.2")
    void setBreed(boolean breed);
}
