package foton;

/** Daemon polling bridge for ticket-backed chunk futures. */
final class FotonChunkRequests {
    private static final java.util.concurrent.ScheduledExecutorService EXECUTOR =
        java.util.concurrent.Executors.newSingleThreadScheduledExecutor(r -> {
            Thread t = new Thread(r, "foton-chunk-futures"); t.setDaemon(true); return t;
        });
    private FotonChunkRequests() {}
    static void watch(String request, Runnable ready) {
        EXECUTOR.scheduleWithFixedDelay(() -> {
            if (!Native.chunkRequestReady(request)) return;
            ready.run();
        }, 0L, 10L, java.util.concurrent.TimeUnit.MILLISECONDS);
    }
}
