package io.papermc.paper.entity;

import org.jspecify.annotations.NullMarked;

/** Paper entity that can be force-sheared. */
@NullMarked
public interface Shearable extends org.bukkit.entity.Entity {
    default void shear() { shear(net.kyori.adventure.sound.Sound.Source.PLAYER); }
    void shear(net.kyori.adventure.sound.Sound.Source source);
    boolean readyToBeSheared();
}
