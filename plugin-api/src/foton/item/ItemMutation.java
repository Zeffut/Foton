package foton.item;

/** Owns the immutable base through native validation and the complete destination commit. */
public final class ItemMutation {
    public static ItemMutation empty() { return new ItemMutation(null, "", new String[0], new String[0]); }
    private final NativeItemLease lease;
    private final String projection;
    private final String[] keys;
    private final String[] operations;

    ItemMutation(NativeItemLease lease, String projection, String[] keys, String[] operations) {
        this.lease = lease;
        this.projection = java.util.Objects.requireNonNull(projection, "item projection");
        this.keys = keys.clone();
        this.operations = operations.clone();
        if (keys.length != operations.length) throw new IllegalArgumentException("item edit lengths");
    }
}
