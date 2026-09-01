package io.papermc.paper;

import java.util.OptionalInt;

/** Build metadata exposed by Paper's compatibility API. */
public final class ServerBuildInfo {
    private static final ServerBuildInfo INSTANCE = new ServerBuildInfo();

    private ServerBuildInfo() {}

    /** Returns the immutable metadata for this Steel server build. */
    public static ServerBuildInfo buildInfo() {
        return INSTANCE;
    }

    /** Steel does not assign Paper's numeric build sequence. */
    public OptionalInt buildNumber() {
        return OptionalInt.empty();
    }

    /** Minecraft data version targeted by this server. */
    public String minecraftVersionId() {
        return "26.2";
    }
}
