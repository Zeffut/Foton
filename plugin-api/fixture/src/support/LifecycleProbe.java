package fixture;

import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;

/** Parent-loaded lifecycle observations that never retain a plugin object or class. */
public final class LifecycleProbe {
    private static final Map<String, AtomicInteger> calls = new ConcurrentHashMap<>();
    private static final Map<String, Boolean> flags = new ConcurrentHashMap<>();
    private static final AtomicInteger followUpAttempts = new AtomicInteger();
    private static final AtomicInteger followUpRejections = new AtomicInteger();
    private static final AtomicInteger followUpRuns = new AtomicInteger();
    private static volatile CountDownLatch asyncStarted = new CountDownLatch(1);
    private static volatile CountDownLatch releaseAsync = new CountDownLatch(1);
    private static volatile CountDownLatch asyncFinished = new CountDownLatch(1);

    private LifecycleProbe() {}

    public static void reset() {
        calls.clear();
        flags.clear();
        followUpAttempts.set(0);
        followUpRejections.set(0);
        followUpRuns.set(0);
        asyncStarted = new CountDownLatch(1);
        releaseAsync = new CountDownLatch(1);
        asyncFinished = new CountDownLatch(1);
    }

    public static void called(String name) {
        calls.computeIfAbsent(name, ignored -> new AtomicInteger()).incrementAndGet();
    }

    public static int calls(String name) {
        AtomicInteger count = calls.get(name);
        return count == null ? 0 : count.get();
    }

    public static void flag(String name, boolean value) {
        flags.put(name, value);
    }

    public static boolean flag(String name) {
        return Boolean.TRUE.equals(flags.get(name));
    }

    public static void loaderOwned(String name, ClassLoader loader) {
        flag(name, loader instanceof org.bukkit.plugin.java.PluginClassLoader);
    }

    public static void asyncStarted() {
        asyncStarted.countDown();
    }

    public static void awaitAsyncStarted() {
        await(asyncStarted, "async rollback task did not start");
    }

    public static void awaitRelease() {
        await(releaseAsync, "async rollback task was not released");
    }

    public static void releaseAsync() {
        releaseAsync.countDown();
    }

    public static void asyncFinished() {
        asyncFinished.countDown();
    }

    public static void awaitAsyncFinished() {
        await(asyncFinished, "async rollback task did not finish");
    }

    public static void followUpAttempt(boolean rejected) {
        followUpAttempts.incrementAndGet();
        if (rejected) followUpRejections.incrementAndGet();
    }

    public static int followUpAttempts() {
        return followUpAttempts.get();
    }

    public static int followUpRejections() {
        return followUpRejections.get();
    }

    public static void followUpRan() {
        followUpRuns.incrementAndGet();
    }

    public static int followUpRuns() {
        return followUpRuns.get();
    }

    private static void await(CountDownLatch latch, String message) {
        try {
            if (!latch.await(5, TimeUnit.SECONDS)) throw new AssertionError(message);
        } catch (InterruptedException interrupted) {
            Thread.currentThread().interrupt();
            throw new AssertionError(message, interrupted);
        }
    }
}
