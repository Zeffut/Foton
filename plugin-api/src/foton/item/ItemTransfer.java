package foton.item;

/** JNI result owns the snapshot even before a Bukkit stack has been hydrated. */
public final class ItemTransfer {
    private final NativeItemLease lease;
    private final String projection;

    private ItemTransfer(String epoch, String id, String projection) {
        this.lease = new NativeItemLease(epoch, id);
        this.projection = java.util.Objects.requireNonNull(projection);
    }

    public NativeItemLease lease() { return lease; }
    public String projection() { return projection; }

    /** This snapshot as an edit-free input, to say which stack a write expects to replace. */
    public ItemMutation unchanged() { return new ItemMutation(lease, projection, new String[0], new String[0]); }
}
