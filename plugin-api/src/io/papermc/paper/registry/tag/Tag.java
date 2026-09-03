package io.papermc.paper.registry.tag;

import java.util.Collection;
import io.papermc.paper.registry.TypedKey;

/** A named group of registry entries, as Paper hands one to a plugin. */
public interface Tag<T> {
    TagKey<T> tagKey();

    Collection<TypedKey<T>> values();

    boolean contains(TypedKey<T> key);
}
