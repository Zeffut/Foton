package io.papermc.paper.tag;

import java.util.Collection;
import java.util.LinkedHashMap;
import java.util.Map;
import io.papermc.paper.registry.TypedKey;
import io.papermc.paper.registry.tag.SimpleTag;
import io.papermc.paper.registry.tag.TagKey;

/** Where a plugin adds to a tag after vanilla's tags have been flattened.
 *
 * "Flattened" is Paper's word for the point where a tag that refers to other
 * tags has become a plain list of entries. Adding here means adding to that
 * list, which is why a plugin can put its own enchantment into
 * `#minecraft:enchantable/mining` and have the game treat it as one.
 *
 * What is added is kept and readable. It is not yet pushed into Foton's own
 * tag registry -- that registry is built and frozen before any plugin loads,
 * so making this take effect is a change to when Foton freezes, not a change
 * here.
 */
public final class PostFlattenTagRegistrar<T> {
    private final Map<TagKey<T>, SimpleTag<T>> tags = new LinkedHashMap<>();

    public void addToTag(TagKey<T> key, Collection<? extends TypedKey<T>> values) {
        if (key == null || values == null) {
            return;
        }
        tags.computeIfAbsent(key, SimpleTag::new).addAll(values);
    }

    public SimpleTag<T> getTag(TagKey<T> key) {
        return tags.computeIfAbsent(key, SimpleTag::new);
    }

    public Map<TagKey<T>, SimpleTag<T>> tags() {
        return tags;
    }
}
