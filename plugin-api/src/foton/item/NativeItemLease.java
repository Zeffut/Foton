package foton.item;

import java.lang.ref.Cleaner;
import foton.Native;

/** Process-local ownership only; never part of item persistence. */
public final class NativeItemLease {
    private static final Cleaner CLEANER = Cleaner.create();
    private final String epoch;
    private final String id;
    private final Cleaner.Cleanable cleanup;

    NativeItemLease(String epoch, String id) {
        this.epoch = java.util.Objects.requireNonNull(epoch);
        this.id = java.util.Objects.requireNonNull(id);
        cleanup = CLEANER.register(this, new Release(epoch, id));
    }

    private record Release(String epoch, String id) implements Runnable {
        @Override public void run() { Native.releaseItemLease(epoch, id); }
    }
}
