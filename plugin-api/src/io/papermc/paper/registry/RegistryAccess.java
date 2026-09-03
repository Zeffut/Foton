package io.papermc.paper.registry;

import org.bukkit.Keyed;
import org.bukkit.Registry;

/**
 * Reaches the server's registries by key rather than by class.
 *
 * <p>An interface, as in Paper: seven plugins of the measured corpus call
 * {@link #getRegistry(RegistryKey)} with {@code invokeinterface}, which a class
 * would answer with an {@code IncompatibleClassChangeError}.</p>
 */
public interface RegistryAccess {
    static RegistryAccess registryAccess() {
        return FotonRegistryAccess.INSTANCE;
    }

    <T extends Keyed> Registry<T> getRegistry(Class<T> type);

    <T extends Keyed> Registry<T> getRegistry(RegistryKey<T> key);
}

final class FotonRegistryAccess implements RegistryAccess {
    static final RegistryAccess INSTANCE = new FotonRegistryAccess();

    private FotonRegistryAccess() {}

    @Override
    public <T extends Keyed> Registry<T> getRegistry(Class<T> type) {
        return org.bukkit.Bukkit.getRegistry(type);
    }

    @Override
    @SuppressWarnings("unchecked")
    public <T extends Keyed> Registry<T> getRegistry(RegistryKey<T> key) {
        if (key == RegistryKey.ENCHANTMENT) return (Registry<T>) Registry.ENCHANTMENT;
        return null;
    }
}
