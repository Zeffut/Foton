package foton.item;

/** Independent mutable journal over one shared immutable native referent. */
public final class LiveItemState {
    private final NativeItemLease lease;
    private final ComponentEdits edits;

    public LiveItemState() { this(null, new ComponentEdits()); }
    public LiveItemState(ItemTransfer transfer) { this(transfer.lease(), new ComponentEdits()); }
    private LiveItemState(NativeItemLease lease, ComponentEdits edits) {
        this.lease = lease;
        this.edits = edits;
    }
    public LiveItemState copy() { return new LiveItemState(lease, edits.copy()); }
    public void record(String key, ComponentEdits.Operation operation) { edits.record(key, operation); }
    public boolean hasNativeBase() { return lease != null; }
    public ItemMutation mutation(String projection) {
        return new ItemMutation(lease, projection, edits.keys(), edits.operations());
    }
}
