package org.bukkit;

import java.util.concurrent.CompletableFuture;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.ScheduledFuture;
import java.util.concurrent.TimeUnit;

/**
 * Non-blocking handle for a world requested through {@link WorldCreator}.
 * Polling is performed off the server tick and completion means the world has
 * already been attached to Bukkit's world list.
 */
public final class WorldCreationRequest {
    private static final ScheduledExecutorService POLLER =
        Executors.newSingleThreadScheduledExecutor(r -> {
            Thread thread = new Thread(r, "foton-world-creation-poller");
            thread.setDaemon(true);
            return thread;
        });
    private final long id;
    private final String worldName;
    private final CompletableFuture<World> future = new CompletableFuture<>();
    private ScheduledFuture<?> poller;

    WorldCreationRequest(long id, String worldName) {
        this.id = id;
        this.worldName = worldName;
        poller = POLLER.scheduleWithFixedDelay(this::poll, 0, 50, TimeUnit.MILLISECONDS);
    }

    public long id() { return id; }
    public boolean isDone() { return future.isDone(); }
    public CompletableFuture<World> future() { return future; }

    private void poll() {
        int state = foton.Native.worldCreationState(id);
        if (state == 0) return;
        if (state == 1) {
            World world = Bukkit.getWorld(worldName);
            if (world == null) future.completeExceptionally(
                new IllegalStateException("world creation completed but world is not published: " + worldName));
            else future.complete(world);
        } else if (state == 2) {
            future.completeExceptionally(new IllegalStateException("world creation failed: " + worldName));
        } else {
            future.completeExceptionally(new IllegalStateException("unknown world creation request: " + id));
        }
        if (future.isDone() && poller != null) poller.cancel(false);
    }
}
