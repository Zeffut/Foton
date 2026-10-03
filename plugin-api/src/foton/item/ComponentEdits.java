package foton.item;

import java.util.LinkedHashMap;
import java.util.Objects;

/** An absent key means unchanged, not reset or removal. Never a persistence format. */
public final class ComponentEdits {
    public enum Operation { SET, REMOVE, RESET }
    private final LinkedHashMap<String, Operation> changes;

    public ComponentEdits() { changes = new LinkedHashMap<>(); }
    private ComponentEdits(LinkedHashMap<String, Operation> changes) { this.changes = changes; }
    public ComponentEdits copy() { return new ComponentEdits(new LinkedHashMap<>(changes)); }

    public void record(String key, Operation operation) {
        Objects.requireNonNull(key, "component key");
        Objects.requireNonNull(operation, "component operation");
        if (!changes.containsKey(key) && changes.size() >= 4096)
            throw new IllegalArgumentException("too many item component edits");
        changes.put(key, operation);
    }

    String[] keys() { return changes.keySet().toArray(String[]::new); }
    String[] operations() { return changes.values().stream().map(Enum::name).toArray(String[]::new); }
    public boolean isEmpty() { return changes.isEmpty(); }
}
