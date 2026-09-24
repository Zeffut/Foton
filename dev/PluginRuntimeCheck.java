import com.google.common.util.concurrent.Futures;
import com.google.common.util.concurrent.SettableFuture;

/** Executable linkage check for the server-provided plugin classpath. */
public final class PluginRuntimeCheck {
    private PluginRuntimeCheck() {}

    public static void main(String[] args) throws Exception {
        SettableFuture<String> future = SettableFuture.create();
        future.set("linked");
        if (!"linked".equals(Futures.getDone(future))) {
            throw new AssertionError("Guava future did not complete");
        }
    }
}
