package foton;

/** Daemon polling bridge for ticket-backed chunk futures. */
final class FotonChunkRequests {
    private static final java.util.concurrent.ScheduledExecutorService EXECUTOR =
        java.util.concurrent.Executors.newSingleThreadScheduledExecutor(r -> {
            Thread t = new Thread(r, "foton-chunk-futures"); t.setDaemon(true); return t;
        });
    private FotonChunkRequests() {}
    static void watch(String request, Runnable ready) {
        java.util.concurrent.atomic.AtomicReference<java.util.concurrent.ScheduledFuture<?>> task =
            new java.util.concurrent.atomic.AtomicReference<>();
        task.set(EXECUTOR.scheduleWithFixedDelay(() -> {
            if (!Native.chunkRequestReady(request)) return;
            java.util.concurrent.ScheduledFuture<?> current = task.get();
            if (current != null) current.cancel(false);
            ready.run();
        }, 0L, 10L, java.util.concurrent.TimeUnit.MILLISECONDS));
    }
}
