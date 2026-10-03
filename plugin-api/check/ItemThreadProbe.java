import java.util.concurrent.atomic.AtomicReference;
import org.bukkit.inventory.ItemStack;

/** Loaded by the actual JNI test, not run in the standalone API fixture. */
public final class ItemThreadProbe {
    private ItemThreadProbe() {}

    public static void check(ItemStack item) throws InterruptedException {
        AtomicReference<Throwable> failure = new AtomicReference<>();
        Thread thread = new Thread(() -> {
            try {
                var diagnostic = java.lang.management.ManagementFactory.getPlatformMXBean(
                    com.sun.management.HotSpotDiagnosticMXBean.class);
                System.err.println("item clone thread=" + Thread.currentThread().getName()
                    + " runtime=" + System.getProperty("java.runtime.version")
                    + " platform=" + System.getProperty("os.name") + "/" + System.getProperty("os.arch")
                    + " ThreadStackSizeKiB=" + diagnostic.getVMOption("ThreadStackSize").getValue());
                ItemStack copy = item.clone();
                if (!copy.isSimilar(item)) throw new AssertionError("native clone similarity");
                if (!copy.setItemMeta(copy.getItemMeta())) throw new AssertionError("same-kind metadata transfer");
                if (!copy.isSimilar(item)) throw new AssertionError("native metadata clone lost components");
            } catch (Throwable error) { failure.set(error); }
        }, "canonical-item-java-thread");
        thread.start();
        thread.join(10_000);
        if (thread.isAlive()) throw new AssertionError("native item operation did not return");
        if (failure.get() != null) throw new AssertionError("Java-created thread item operation failed", failure.get());
    }
}
