package io.papermc.paper.registry;

/** Source of the builders a plugin fills in when it registers a registry entry. */
public interface RegistryBuilderFactory<T, B extends RegistryBuilder<T>> {
    /** A builder with nothing set. */
    B empty();

    /** A builder pre-filled from an existing entry. */
    B copyFrom(TypedKey<T> key);
}
