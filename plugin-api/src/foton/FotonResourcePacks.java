package foton;

import java.util.Map;
import java.util.Objects;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;
import net.kyori.adventure.audience.Audience;
import net.kyori.adventure.resource.ResourcePackCallback;
import net.kyori.adventure.resource.ResourcePackInfo;
import net.kyori.adventure.resource.ResourcePackRequest;
import net.kyori.adventure.resource.ResourcePackStatus;
import org.bukkit.event.player.PlayerResourcePackStatusEvent.Status;

/** The server side of a player's resource packs: what is sent, what is
 * taken back, and who is waiting to hear how a request ended.
 *
 * <p>A player handle is only a UUID, so the callbacks Adventure lets a request
 * carry live here, by player and then by pack, until the client gives a
 * terminal answer for that pack. */
final class FotonResourcePacks {
    /** Vanilla's {@code ClientboundResourcePackPushPacket.MAX_HASH_LENGTH}. */
    private static final int MAX_HASH_LENGTH = 40;
    /** Vanilla's {@code ByteBufCodecs.STRING_UTF8}, which is {@code stringUtf8(32767)}. */
    private static final int MAX_URL_LENGTH = 32767;
    private static final Map<UUID, Map<UUID, ResourcePackCallback>> CALLBACKS = new ConcurrentHashMap<>();

    private FotonResourcePacks() { }

    /** The SHA-1 a legacy caller passes as bytes, as the hex the packet carries. */
    static String hex(byte[] hash) {
        if (hash == null) return "";
        if (hash.length != 20)
            throw new IllegalArgumentException("Resource pack hash should be 20 bytes long but was " + hash.length);
        return java.util.HexFormat.of().formatHex(hash);
    }

    /** Pushes each pack of the request, after dropping the player's current ones if it replaces them. */
    static void send(UUID player, ResourcePackRequest request, Audience audience) {
        Objects.requireNonNull(request, "request");
        for (ResourcePackInfo pack : request.packs()) {
            if (pack.uri().toASCIIString().length() > MAX_URL_LENGTH)
                throw new IllegalArgumentException("Resource pack URL is too long (max " + MAX_URL_LENGTH + ")");
            if (pack.hash().length() > MAX_HASH_LENGTH)
                throw new IllegalArgumentException(
                    "Hash is too long (max " + MAX_HASH_LENGTH + ", was " + pack.hash().length() + ")");
        }
        if (request.replace()) {
            CALLBACKS.remove(player);
            Native.removeResourcePacks(player.toString(), null);
        }
        String prompt = request.prompt() == null ? null : FotonComponents.toJson(request.prompt());
        boolean tracked = request.callback() != ResourcePackCallback.noOp();
        for (ResourcePackInfo pack : request.packs()) {
            // A second request for the same pack takes over its callback.
            if (tracked) CALLBACKS.computeIfAbsent(player, ignored -> new ConcurrentHashMap<>())
                .put(pack.id(), request.callback());
            Native.sendResourcePack(player.toString(), pack.id().toString(), pack.uri().toASCIIString(),
                pack.hash(), request.required(), prompt);
        }
    }

    /** Takes one pack back from the client, or all of them when {@code pack} is null. */
    static void remove(UUID player, UUID pack) {
        if (pack == null) CALLBACKS.remove(player);
        else CALLBACKS.computeIfPresent(player, (ignored, packs) -> {
            packs.remove(pack);
            return packs.isEmpty() ? null : packs;
        });
        Native.removeResourcePacks(player.toString(), pack == null ? null : pack.toString());
    }

    /** Tells whoever sent the pack how the client got on with it. A terminal
     * answer ends the exchange, so the callback is dropped with it. */
    static void received(UUID player, UUID pack, Status status) {
        Map<UUID, ResourcePackCallback> packs = CALLBACKS.get(player);
        ResourcePackCallback callback = packs == null ? null
            : status.isTerminal() ? packs.remove(pack) : packs.get(pack);
        if (packs != null && packs.isEmpty()) CALLBACKS.remove(player, packs);
        if (callback != null)
            callback.packEventReceived(pack, ResourcePackStatus.valueOf(status.name()), new FotonPlayer(player));
    }

    /** The player left; nothing they were sent can be answered any more. */
    static void forget(UUID player) {
        CALLBACKS.remove(player);
    }
}
