package foton;

/** Chunk futures waiting on the server, settled from the tick so that a
 * plugin's callbacks run on the main thread as they do under Paper. */
final class FotonChunkRequests {
    private record Pending(String request, java.util.concurrent.CompletableFuture<org.bukkit.Chunk> future,
            java.util.function.Supplier<org.bukkit.Chunk> chunk) {}

    private static final java.util.concurrent.ConcurrentLinkedQueue<Pending> PENDING =
        new java.util.concurrent.ConcurrentLinkedQueue<>();

    private FotonChunkRequests() {}

    static void watch(String request, java.util.concurrent.CompletableFuture<org.bukkit.Chunk> future,
            java.util.function.Supplier<org.bukkit.Chunk> chunk) {
        PENDING.add(new Pending(request, future, chunk));
    }

    /** Completes the futures whose chunk is now fully loaded. Called once per tick. */
    static void tick() {
        PENDING.removeIf(pending -> {
            if (pending.future.isDone()) return true;
            if (!Native.chunkRequestReady(pending.request)) return false;
            pending.future.complete(pending.chunk.get());
            return true;
        });
    }
}
