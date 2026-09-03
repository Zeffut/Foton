package io.papermc.paper.registry.event;

import io.papermc.paper.registry.RegistryKey;
import io.papermc.paper.registry.data.EnchantmentRegistryEntry;
import org.bukkit.enchantments.Enchantment;

/** The registries a plugin can hook while they are still open. */
public final class RegistryEvents {
    private RegistryEvents() {}

    /** Enchantments. The one registry plugins actually add to. */
    public static final RegistryEventProvider<Enchantment, EnchantmentRegistryEntry.Builder>
        ENCHANTMENT = new RegistryEventProvider<>(RegistryKey.ENCHANTMENT);
}
