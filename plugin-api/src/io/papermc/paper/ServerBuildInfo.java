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

    /** Whether this server answers to a brand a plugin is asking about.
     *
     * Plugins use this to find out whether they are on Paper. Foton serves
     * Paper's API, so it says yes to Paper's key and to its own, and no to
     * anything else -- a plugin asking for Folia's brand wants to know
     * whether regions are threaded, and here they are not.
     */
    public boolean isBrandCompatible(net.kyori.adventure.key.Key brand) {
        if (brand == null) {
            return false;
        }
        String name = brand.asString();
        return "papermc:paper".equals(name) || "foton:foton".equals(name);
    }

    /** Minecraft data version targeted by this server. */
    public String minecraftVersionId() {
        return "26.2";
    }
}
