package foton.network;

/** Entry point used through JNI to create one Via pipeline per TCP connection. */
public final class FotonViaBridge {
    private FotonViaBridge() {}

    /** Returns null when no plugin registered a Paper channel listener. */
    public static FotonViaChannel open() {
        if (io.papermc.paper.network.ChannelInitializeListenerHolder.getListeners().isEmpty()) {
            return null;
        }
        return new FotonViaChannel();
    }
}
