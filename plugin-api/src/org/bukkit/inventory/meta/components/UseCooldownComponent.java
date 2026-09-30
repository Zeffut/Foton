package org.bukkit.inventory.meta.components;

import org.bukkit.NamespacedKey;
import org.bukkit.configuration.serialization.ConfigurationSerializable;

/** The vanilla {@code use_cooldown} component: how long using the item locks it,
 * and which group of items shares that lock. */
public interface UseCooldownComponent extends ConfigurationSerializable {
    float getCooldownSeconds();

    /** Vanilla requires a positive duration. */
    void setCooldownSeconds(float cooldown);

    /** The group sharing the cooldown; null means the item's own type. */
    NamespacedKey getCooldownGroup();

    void setCooldownGroup(NamespacedKey group);
}
