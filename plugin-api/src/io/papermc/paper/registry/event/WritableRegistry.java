package io.papermc.paper.registry.event;

import io.papermc.paper.registry.RegistryBuilder;
import io.papermc.paper.registry.RegistryBuilderFactory;
import io.papermc.paper.registry.TypedKey;
import io.papermc.paper.registry.data.EnchantmentRegistryEntry;
import java.util.function.Consumer;

/** A registry that accepts entries during Paper's registry lifecycle. */
public interface WritableRegistry<T, B extends RegistryBuilder<T>> {
    default void register(TypedKey<T> key, Consumer<? super B> consumer) {
        registerWith(key, factory -> {
            B builder = factory.empty();
            consumer.accept(builder);
            if (builder instanceof EnchantmentRegistryEntry.Builder enchantmentBuilder) {
                PluginEnchantmentQueue.queue_plugin_enchantment(key, enchantmentBuilder);
            }
        });
    }

    void registerWith(TypedKey<T> key, Consumer<RegistryBuilderFactory<T, B>> consumer);
}
