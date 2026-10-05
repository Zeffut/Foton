import java.util.concurrent.atomic.AtomicReference;

/** Invoked only by the isolated native brewing fixture, with ordinary JVM thread stacks. */
public final class BrewingGetterStackCheck {
    private BrewingGetterStackCheck() {}
    private static final java.util.concurrent.atomic.AtomicBoolean REPORTED = new java.util.concurrent.atomic.AtomicBoolean();

    public static byte[] get(String world, int x, int y, int z, long identity, int slot, boolean live)
            throws Exception {
        var result = new AtomicReference<byte[]>();
        var failure = new AtomicReference<Throwable>();
        Thread thread = new Thread(() -> {
            try {
                if (REPORTED.compareAndSet(false, true)) {
                    var bean = java.lang.management.ManagementFactory.getPlatformMXBean(com.sun.management.HotSpotDiagnosticMXBean.class);
                    System.out.println("brewing getter thread=" + Thread.currentThread().getName()
                        + " runtime=" + Runtime.version() + " ThreadStackSizeKiB=" + bean.getVMOption("ThreadStackSize").getValue());
                }
                result.set(live ? foton.Native.brewingStandLiveItem(world, x, y, z, identity, slot)
                    : foton.Native.brewingStandSnapshot(world, x, y, z));
            } catch (Throwable error) { failure.set(error); }
        }, "brewing-default-java-thread");
        thread.setDaemon(true);
        thread.start();
        thread.join(10_000);
        if (thread.isAlive()) throw new AssertionError("brewing Java-thread getter timed out");
        if (failure.get() != null) throw new AssertionError("brewing Java-thread getter failed", failure.get());
        return result.get();
    }
}
