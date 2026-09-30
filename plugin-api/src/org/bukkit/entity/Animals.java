package org.bukkit.entity;

import java.util.UUID;
import org.bukkit.Material;
import org.bukkit.inventory.ItemStack;

/** Paper's common contract for breeding animals. */
public interface Animals extends Breedable {
    UUID getBreedCause();
    void setBreedCause(UUID breedCause);
    boolean isLoveMode();
    int getLoveModeTicks();
    void setLoveModeTicks(int ticks);
    boolean isBreedItem(ItemStack stack);
    boolean isBreedItem(Material material);
}
