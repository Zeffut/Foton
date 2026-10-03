package foton;

import java.util.AbstractSet;
import java.util.Collection;
import java.util.Iterator;
import java.util.NoSuchElementException;
import java.util.Objects;

/** A live Bukkit Set view whose mutations change the entity's persisted native tags.
 * Paper 1.21.11 exposes a 1024-entry SizeLimitedSet; Foton enforces the same
 * bound in native entity state. */
final class FotonScoreboardTags extends AbstractSet<String> {
    private static final int MAX_TAGS = 1024;
    private final String uuid;

    FotonScoreboardTags(String uuid) { this.uuid = uuid; }

    private String[] snapshot() {
        String[] tags = Native.entityScoreboardTags(uuid);
        return tags == null ? new String[0] : tags;
    }

    @Override public int size() { return snapshot().length; }

    @Override public boolean contains(Object value) {
        if (!(value instanceof String tag)) return false;
        for (String current : snapshot()) if (current.equals(tag)) return true;
        return false;
    }

    @Override public boolean add(String tag) {
        return Native.entityAddScoreboardTag(uuid, Objects.requireNonNull(tag, "tag"));
    }

    @Override public boolean addAll(Collection<? extends String> tags) {
        Objects.requireNonNull(tags, "tags");
        // Paper's SizeLimitedSet rejects the entire batch at this boundary,
        // even when the input contains tags already present in the set.
        if (size() + tags.size() >= MAX_TAGS) return false;
        boolean changed = false;
        for (String tag : tags) changed |= add(tag);
        return changed;
    }

    @Override public boolean remove(Object value) {
        return value instanceof String tag && Native.entityRemoveScoreboardTag(uuid, tag);
    }

    @Override public Iterator<String> iterator() {
        // JNI supplies a snapshot; Paper iterates its live ObjectOpenHashSet.
        // Iterator removals still write through to native entity state.
        String[] tags = snapshot();
        return new Iterator<>() {
            private int index;
            private String current;
            private boolean canRemove;

            @Override public boolean hasNext() { return index < tags.length; }

            @Override public String next() {
                if (!hasNext()) throw new NoSuchElementException();
                current = tags[index++];
                canRemove = true;
                return current;
            }

            @Override public void remove() {
                if (!canRemove) throw new IllegalStateException();
                Native.entityRemoveScoreboardTag(uuid, current);
                canRemove = false;
            }
        };
    }
}
