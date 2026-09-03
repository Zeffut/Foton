package io.papermc.paper.registry.tag;

import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.List;
import io.papermc.paper.registry.TypedKey;

/** A tag a plugin is building, before anything is frozen. */
public final class SimpleTag<T> implements Tag<T> {
    private final TagKey<T> key;
    private final List<TypedKey<T>> values = new ArrayList<>();

    public SimpleTag(TagKey<T> key) {
        this.key = key;
    }

    @Override
    public TagKey<T> tagKey() {
        return key;
    }

    @Override
    public Collection<TypedKey<T>> values() {
        return Collections.unmodifiableList(values);
    }

    @Override
    public boolean contains(TypedKey<T> entry) {
        return values.contains(entry);
    }

    /** Adds entries a plugin asked for. Duplicates are dropped, as a tag is a set. */
    public void addAll(Collection<? extends TypedKey<T>> entries) {
        for (TypedKey<T> entry : entries) {
            if (entry != null && !values.contains(entry)) {
                values.add(entry);
            }
        }
    }
}
