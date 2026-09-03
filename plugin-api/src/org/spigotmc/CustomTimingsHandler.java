package org.spigotmc;

import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

/**
 * Lightweight Spigot-compatible custom timing section.
 *
 * Timing values are measured with {@link System#nanoTime()}, so elapsed
 * durations are monotonic and are not affected by wall-clock adjustments.
 */
public final class CustomTimingsHandler {
    private static final Map<String, CustomTimingsHandler> HANDLERS = new ConcurrentHashMap<>();
    private final String name;
    private final CustomTimingsHandler parent;
    private long timingStart;
    private long totalTime;
    private long timingCount;
    private long currentTickTotal;
    private int timingDepth;

    public CustomTimingsHandler(String name) {
        this(name, null);
    }

    public CustomTimingsHandler(String name, CustomTimingsHandler parent) {
        if (name == null) throw new NullPointerException("name");
        this.name = name;
        this.parent = parent;
        HANDLERS.putIfAbsent(name, this);
    }

    public static CustomTimingsHandler getHandler(String name) {
        if (name == null) throw new NullPointerException("name");
        return HANDLERS.computeIfAbsent(name, CustomTimingsHandler::new);
    }

    public static boolean isEnabled() {
        return true;
    }

    public static void tick() {
        for (CustomTimingsHandler handler : HANDLERS.values()) {
            synchronized (handler) {
                handler.currentTickTotal = 0L;
            }
        }
    }

    public synchronized void startTiming() {
        if (timingDepth++ == 0) {
            timingStart = System.nanoTime();
        }
        if (parent != null && timingDepth == 1) {
            parent.startTiming();
        }
    }

    public synchronized void stopTiming() {
        if (timingDepth == 0) return;
        if (--timingDepth != 0) return;
        long elapsed = System.nanoTime() - timingStart;
        if (elapsed < 0) elapsed = 0;
        totalTime += elapsed;
        currentTickTotal += elapsed;
        timingCount++;
        timingStart = 0L;
        if (parent != null) parent.stopTiming();
    }

    public synchronized void abort() {
        if (timingDepth == 0) return;
        timingDepth = 0;
        timingStart = 0L;
        if (parent != null) parent.abort();
    }

    public synchronized void reset() {
        timingStart = 0L;
        totalTime = 0L;
        timingCount = 0L;
        currentTickTotal = 0L;
        timingDepth = 0;
    }

    public String getName() { return name; }
    public CustomTimingsHandler getParent() { return parent; }
    public synchronized boolean isTiming() { return timingDepth != 0; }
    public synchronized long getTimingStart() { return timingStart; }
    public synchronized long getTimingCount() { return timingCount; }
    public synchronized long getTotalTime() { return totalTime; }
    public synchronized long getCurTickTotal() { return currentTickTotal; }

    /** Spigot compatibility alias. */
    public synchronized long getCount() { return timingCount; }
}
