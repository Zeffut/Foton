import io.papermc.paper.datacomponent.item.ResolvableProfile;
import java.util.UUID;

/** Runs the Paper-compiled consumer with Foton's API jar. */
final class FotonResolvableProfileBinaryRunner {
    private FotonResolvableProfileBinaryRunner() {}

    public static void main(String[] args) {
        PaperResolvableProfileConsumer.verify(ResolvableProfile.resolvableProfile()
            .uuid(UUID.fromString("9181f3c6-8d64-4f6c-9a95-d7d92f365a1f"))
            .name("paper-binary-consumer"));
    }
}
