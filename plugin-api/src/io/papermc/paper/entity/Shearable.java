package io.papermc.paper.entity;

/** Paper entity that can be force-sheared. */
public interface Shearable extends org.bukkit.entity.Entity {
    default void shear() { shear(net.kyori.adventure.sound.Sound.Source.PLAYER); }
    void shear(net.kyori.adventure.sound.Sound.Source source);
    boolean readyToBeSheared();
}
