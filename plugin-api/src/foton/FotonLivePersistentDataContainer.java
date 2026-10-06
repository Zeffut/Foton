package foton;

import java.io.IOException;
import java.util.Arrays;
import java.util.Set;
import java.util.UUID;
import org.bukkit.NamespacedKey;
import org.bukkit.persistence.PersistentDataContainer;
import org.bukkit.persistence.PersistentDataType;

/** A container whose data lives on the server side -- an entity's, a player's or a chunk's.
 *
 * <p>Every read first refreshes from the owner and every change is written back at once,
 * so any number of handles to the same owner see the same data and nothing needs an
 * {@code update()} call. The owner saves it with its own NBT, under the key Paper uses.</p>
 */
final class FotonLivePersistentDataContainer extends FotonPersistentDataContainer {
    /** Where the data lives; {@code pull} answers null when the owner is gone. */
    interface Source {
        byte[] pull();
        void push(byte[] encoded);
    }

    private final Source source;
    private byte[] synced;
    private boolean syncing;

    private FotonLivePersistentDataContainer(Source source) {
        this.source = source;
        setMutationListener(this::push);
    }

    /** An entity's or player's container, kept as {@code BukkitValues} in its saved NBT. */
    static FotonLivePersistentDataContainer ofEntity(UUID id) {
        String key = id.toString();
        return new FotonLivePersistentDataContainer(new Source() {
            @Override public byte[] pull() { return Native.entityPersistentData(key); }
            @Override public void push(byte[] encoded) { Native.setEntityPersistentData(key, encoded); }
        });
    }

    /** A chunk's container, kept as {@code ChunkBukkitValues} in the chunk save. */
    static FotonLivePersistentDataContainer ofChunk(String world, int x, int z) {
        return new FotonLivePersistentDataContainer(new Source() {
            @Override public byte[] pull() { return Native.chunkPersistentData(world, x, z); }
            @Override public void push(byte[] encoded) {
                if (!Native.setChunkPersistentData(world, x, z, encoded))
                    throw new IllegalStateException("chunk " + x + ", " + z + " of " + world
                        + " is not loaded, so its persistent data cannot be changed");
            }
        });
    }

    private void refresh() {
        if (syncing) return;
        byte[] bytes = source.pull();
        if (bytes == null || Arrays.equals(bytes, synced)) return;
        syncing = true;
        try {
            super.readFromBytes(bytes, true);
            synced = bytes;
        } catch (IOException corrupt) {
            // Keep what this handle holds rather than fail a read.
        } finally {
            syncing = false;
        }
    }

    private void push() {
        if (syncing) return;
        byte[] encoded;
        try {
            encoded = Snbt.toBinary(super.toNbt());
        } catch (IOException error) {
            throw new IllegalStateException("persistent data cannot be encoded", error);
        }
        source.push(encoded);
        synced = encoded;
    }

    @Override
    public <P, C> void set(NamespacedKey key, PersistentDataType<P, C> type, C value) {
        refresh();
        super.set(key, type, value);
    }

    @Override
    public <P, C> C get(NamespacedKey key, PersistentDataType<P, C> type) {
        refresh();
        return super.get(key, type);
    }

    @Override
    public <P, C> boolean has(NamespacedKey key, PersistentDataType<P, C> type) {
        refresh();
        return super.has(key, type);
    }

    @Override public boolean has(NamespacedKey key) { refresh(); return super.has(key); }
    @Override public void remove(NamespacedKey key) { refresh(); super.remove(key); }
    @Override public Set<NamespacedKey> getKeys() { refresh(); return super.getKeys(); }
    @Override public boolean isEmpty() { refresh(); return super.isEmpty(); }
    @Override public int getSize() { refresh(); return super.getSize(); }

    @Override
    public void copyTo(PersistentDataContainer other, boolean replace) {
        refresh();
        if (other instanceof FotonLivePersistentDataContainer live) live.refresh();
        super.copyTo(other, replace);
    }

    @Override
    public void readFromBytes(byte[] bytes, boolean clear) throws IOException {
        refresh();
        super.readFromBytes(bytes, clear);
    }

    @Override
    public java.util.Map<String, Object> toNbt() {
        refresh();
        return super.toNbt();
    }
}
