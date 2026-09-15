import io.papermc.paper.datacomponent.item.ResolvableProfile;
import java.util.UUID;

/**
 * Compiled against Paper 26.2, then run with Foton's API jar in Paper's place.
 * Catches the realistic mutation where Builder becomes a concrete class or its
 * inherited generic build descriptor stops linking to Paper-compiled plugins.
 */
final class PaperResolvableProfileConsumer {
    private PaperResolvableProfileConsumer() {}

    static void verify(ResolvableProfile.Builder builder) {
        UUID expectedId = UUID.fromString("9181f3c6-8d64-4f6c-9a95-d7d92f365a1f");
        ResolvableProfile profile = builder.build();

        if (!expectedId.equals(profile.uuid())) {
            throw new AssertionError("Paper-compiled consumer lost the profile UUID");
        }
        if (!"paper-binary-consumer".equals(profile.name())) {
            throw new AssertionError("Paper-compiled consumer lost the profile name");
        }
    }
}
