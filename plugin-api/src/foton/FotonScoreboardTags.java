package foton;

import java.util.AbstractSet;
import java.util.Arrays;
import java.util.Iterator;

/** An entity's scoreboard tags, as the live set Paper hands out.
 *
 * <p>Adding to or removing from it changes the entity's own tags -- the ones
 * {@code /tag} lists and the entity saves -- because plugins write
 * {@code entity.getScoreboardTags().add(...)} as often as they call
 * {@code addScoreboardTag}. Iteration walks a snapshot. */
public final class FotonScoreboardTags extends AbstractSet<String> {
    private final String entity;

    public FotonScoreboardTags(String entity) { this.entity = entity; }

    private String[] snapshot() {
        String[] tags = Native.entityTags(entity);
        return tags == null ? new String[0] : tags;
    }

    @Override public int size() { return snapshot().length; }
    @Override public boolean contains(Object tag) { return tag instanceof String && Arrays.asList(snapshot()).contains(tag); }
    @Override public boolean add(String tag) { return tag != null && Native.addEntityTag(entity, tag); }
    @Override public boolean remove(Object tag) { return tag instanceof String value && Native.removeEntityTag(entity, value); }

    @Override public Iterator<String> iterator() {
        Iterator<String> tags = Arrays.asList(snapshot()).iterator();
        return new Iterator<>() {
            private String current;
            @Override public boolean hasNext() { return tags.hasNext(); }
            @Override public String next() { current = tags.next(); return current; }
            @Override public void remove() {
                if (current == null) throw new IllegalStateException();
                Native.removeEntityTag(entity, current);
                current = null;
            }
        };
    }
}
