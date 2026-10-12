import java.lang.reflect.*;
import java.util.UUID;

/**
 * Walks {@code CraftPlayer#getHandle()} the way ViaVersion's {@code JoinListener} does: by the simple
 * names of the field types, from the handle to the packet listener to the connection to the Netty
 * channel. A change to those names, or a channel that stays bound after it closes, fails here
 * instead of as a warning in a plugin's log (issue #41).
 */
public class ViaPlayerHandleCheck {
    static Field find(boolean sup, Class<?> c, String... names) throws Exception {
        for (Field f : c.getDeclaredFields())
            for (String n : names)
                if (f.getType().getSimpleName().equals(n)) { f.setAccessible(true); return f; }
        if (sup && c.getSuperclass() != null && c != Object.class) return find(true, c.getSuperclass(), names);
        throw new NoSuchFieldException(names[0]);
    }
    static Field byType(Class<?> c, Class<?> t) throws Exception {
        for (Field f : c.getDeclaredFields()) if (f.getType() == t) { f.setAccessible(true); return f; }
        throw new NoSuchFieldException(t.getSimpleName());
    }
    public static void main(String[] a) throws Exception {
        // Via only gets a channel when a plugin registered a Paper channel listener.
        io.papermc.paper.network.ChannelInitializeListenerHolder.addListener(
            net.kyori.adventure.key.Key.key("foton", "via_player_handle_check"), ch -> ch.pipeline().addLast("via-handle-check", new io.netty.channel.ChannelInboundHandlerAdapter()));
        foton.network.FotonViaChannel via = foton.network.FotonViaBridge.open();
        if (via == null) throw new AssertionError("open() returned null");
        UUID id = UUID.randomUUID();
        foton.network.FotonViaBridge.bind(id.toString(), via);

        // The static initializer of JoinListener, verbatim in shape.
        Method getHandle = Class.forName("foton.entity.CraftPlayer").getDeclaredMethod("getHandle");
        Field conn = find(false, getHandle.getReturnType(), "PlayerConnection", "ServerGamePacketListenerImpl");
        Field nm = find(true, conn.getType(), "NetworkManager", "Connection");
        Field ch = byType(nm.getType(), Class.forName("io.netty.channel.Channel"));

        // getChannel(player), with the handle built the way CraftPlayer builds it.
        Object handle = new foton.network.ServerPlayerHandle(id);
        Object channel = ch.get(nm.get(conn.get(handle)));
        if (!(channel instanceof io.netty.channel.Channel c) || !c.isOpen()) throw new AssertionError("no open channel: " + channel);
        if (c.pipeline().get("via-handle-check") == null) throw new AssertionError("not the pipeline the listeners initialized");

        // A player with no bound channel yields a handle with a null channel, not an exception.
        Object unbound = ch.get(nm.get(conn.get(new foton.network.ServerPlayerHandle(UUID.randomUUID()))));
        if (unbound != null) throw new AssertionError("unbound player has a channel");

        via.close();
        Object closed = ch.get(nm.get(conn.get(new foton.network.ServerPlayerHandle(id))));
        if (closed != null) throw new AssertionError("a closed channel is still bound");
        System.out.println("ViaPlayerHandleCheck OK");
    }
}
