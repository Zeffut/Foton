package io.papermc.paper.registry;

/**
 * Marker for a builder that produces one entry of a registry.
 *
 * <p>Empty in Paper too: it exists to bind {@code B} in
 * {@link RegistryBuilderFactory} and {@link io.papermc.paper.registry.event.WritableRegistry}
 * to a builder for the right registry.</p>
 */
public interface RegistryBuilder<T> {
}
