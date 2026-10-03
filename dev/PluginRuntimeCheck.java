import com.google.common.util.concurrent.Futures;
import com.google.common.util.concurrent.SettableFuture;
import net.kyori.adventure.text.minimessage.MiniMessage;
import net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer;

/** Executable linkage check for the server-provided plugin classpath. */
public final class PluginRuntimeCheck {
    private PluginRuntimeCheck() {}

    public static void main(String[] args) throws Exception {
        SettableFuture<String> future = SettableFuture.create();
        future.set("linked");
        if (!"linked".equals(Futures.getDone(future))) {
            throw new AssertionError("Guava future did not complete");
        }
        String text = PlainTextComponentSerializer.plainText().serialize(
            MiniMessage.miniMessage().deserialize("<red>Zelda</red>"));
        if (!"Zelda".equals(text)) {
            throw new AssertionError("MiniMessage did not deserialize a component");
        }
    }
}
