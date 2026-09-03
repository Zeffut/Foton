package io.papermc.paper.registry.event;

import io.papermc.paper.registry.RegistryBuilder;
import io.papermc.paper.registry.RegistryBuilderFactory;
import io.papermc.paper.registry.TypedKey;
import io.papermc.paper.registry.data.EnchantmentRegistryEntry;
import org.bukkit.enchantments.Enchantment;
import java.util.function.Consumer;

/** A registry that accepts entries during Paper's registry lifecycle. */
public interface WritableRegistry<T, B extends RegistryBuilder<T>> {
    default void register(TypedKey<T> key, Consumer<? super B> consumer) {
        registerWith(key, factory -> {
            B builder = factory.empty();
            consumer.accept(builder);
            if (builder instanceof EnchantmentRegistryEntry.Builder enchantmentBuilder) {
                // The builder's type is the registry's type, so a builder that
                // is an enchantment's proves T is Enchantment and that the key
                // names one. Java cannot see that through the type parameter,
                // and the instanceof above is what makes the cast safe.
                @SuppressWarnings("unchecked")
                TypedKey<Enchantment> enchantmentKey = (TypedKey<Enchantment>) key;
                PluginEnchantmentQueue.queue_plugin_enchantment(enchantmentKey, enchantmentBuilder);
            }
        });
    }

    void registerWith(TypedKey<T> key, Consumer<RegistryBuilderFactory<T, B>> consumer);
}
