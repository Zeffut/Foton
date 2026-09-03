package io.papermc.paper.registry.event;

import io.papermc.paper.plugin.lifecycle.event.registrar.ReloadableRegistrarEvent;
import io.papermc.paper.registry.RegistryBuilder;
import io.papermc.paper.registry.tag.SimpleTag;
import io.papermc.paper.registry.tag.Tag;
import io.papermc.paper.registry.tag.TagKey;

/** The last moment a plugin can add to a registry before it is closed.
 *
 * Vanilla registries are fixed once the game is running; this is the window
 * before that, and it is the only place a plugin's own enchantment can be
 * added at all.
 */
public interface RegistryFreezeEvent<T, B extends RegistryBuilder<T>>
        extends ReloadableRegistrarEvent {
    /** The registry being closed, open for writing until this returns. */
    WritableRegistry<T, B> registry();

    /** The tag under that key, created empty if nothing has claimed it yet. */
    Tag<T> getOrCreateTag(TagKey<T> key);
}
