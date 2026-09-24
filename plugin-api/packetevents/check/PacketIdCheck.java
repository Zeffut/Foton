import com.github.retrooper.packetevents.protocol.packettype.PacketType;
import com.github.retrooper.packetevents.protocol.packettype.PacketTypeCommon;
import com.github.retrooper.packetevents.protocol.player.ClientVersion;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

/** PacketEvents and Foton must agree on every packet id, or nothing works.
 *
 * PacketEvents carries its own id tables per protocol; Foton's come from the
 * packets.json extracted from the vanilla server. A disagreement would not
 * fail loudly: a listener would read a movement packet as something else and
 * act on nonsense. So the build compares them, two ways.
 *
 * Every id PacketEvents assigns at this protocol must be a packet Foton has,
 * and no two PacketEvents types may share one -- which, with the counts equal,
 * means PacketEvents' table is a permutation of Foton's. Then the packets a
 * plugin actually reads are pinned name to name, which the permutation alone
 * would not catch if two neighbours were swapped.
 */
public final class PacketIdCheck {
    private static final List<String> failures = new ArrayList<>();

    public static void main(String[] args) throws Exception {
        JsonObject packets = JsonParser.parseString(Files.readString(Path.of(args[0]))).getAsJsonObject();
        ClientVersion version = ClientVersion.getById(packets.get("version").getAsInt());
        if (version == null || version.getProtocolVersion() != packets.get("version").getAsInt()) {
            System.err.println("PacketEvents does not know protocol " + packets.get("version"));
            System.exit(1);
        }
        JsonArray serverboundPlay = packets.getAsJsonObject("serverbound").getAsJsonArray("play");
        JsonArray clientboundPlay = packets.getAsJsonObject("clientbound").getAsJsonArray("play");
        JsonArray serverboundConfig = packets.getAsJsonObject("serverbound").getAsJsonArray("config");
        JsonArray clientboundConfig = packets.getAsJsonObject("clientbound").getAsJsonArray("config");

        permutation("serverbound play", PacketType.Play.Client.values(), serverboundPlay, version);
        permutation("clientbound play", PacketType.Play.Server.values(), clientboundPlay, version);
        permutation("serverbound config", PacketType.Configuration.Client.values(), serverboundConfig, version);
        permutation("clientbound config", PacketType.Configuration.Server.values(), clientboundConfig, version);

        pin(PacketType.Play.Client.TELEPORT_CONFIRM, serverboundPlay, "accept_teleportation", version);
        pin(PacketType.Play.Client.ATTACK, serverboundPlay, "attack", version);
        pin(PacketType.Play.Client.INTERACT_ENTITY, serverboundPlay, "interact", version);
        pin(PacketType.Play.Client.PLAYER_POSITION, serverboundPlay, "move_player_pos", version);
        pin(PacketType.Play.Client.PLAYER_POSITION_AND_ROTATION, serverboundPlay, "move_player_pos_rot", version);
        pin(PacketType.Play.Client.PLAYER_ROTATION, serverboundPlay, "move_player_rot", version);
        pin(PacketType.Play.Client.PLAYER_FLYING, serverboundPlay, "move_player_status_only", version);
        pin(PacketType.Play.Client.PLAYER_DIGGING, serverboundPlay, "player_action", version);
        pin(PacketType.Play.Client.ENTITY_ACTION, serverboundPlay, "player_command", version);
        pin(PacketType.Play.Client.PLAYER_INPUT, serverboundPlay, "player_input", version);
        pin(PacketType.Play.Client.PONG, serverboundPlay, "pong", version);
        pin(PacketType.Play.Client.HELD_ITEM_CHANGE, serverboundPlay, "set_carried_item", version);
        pin(PacketType.Play.Client.ANIMATION, serverboundPlay, "swing", version);
        pin(PacketType.Play.Client.PLAYER_BLOCK_PLACEMENT, serverboundPlay, "use_item_on", version);
        pin(PacketType.Play.Client.USE_ITEM, serverboundPlay, "use_item", version);
        pin(PacketType.Play.Client.PLUGIN_MESSAGE, serverboundPlay, "custom_payload", version);
        pin(PacketType.Play.Client.CLIENT_TICK_END, serverboundPlay, "client_tick_end", version);
        pin(PacketType.Play.Client.CLICK_WINDOW, serverboundPlay, "container_click", version);
        pin(PacketType.Play.Client.CLOSE_WINDOW, serverboundPlay, "container_close", version);

        pin(PacketType.Play.Server.JOIN_GAME, clientboundPlay, "login", version);
        pin(PacketType.Play.Server.RESPAWN, clientboundPlay, "respawn", version);
        pin(PacketType.Play.Server.ENTITY_METADATA, clientboundPlay, "set_entity_data", version);
        pin(PacketType.Play.Server.ENTITY_VELOCITY, clientboundPlay, "set_entity_motion", version);
        pin(PacketType.Play.Server.EXPLOSION, clientboundPlay, "explode", version);
        pin(PacketType.Play.Server.PLAYER_POSITION_AND_LOOK, clientboundPlay, "player_position", version);
        pin(PacketType.Play.Server.PLAYER_ROTATION, clientboundPlay, "player_rotation", version);
        pin(PacketType.Play.Server.PING, clientboundPlay, "ping", version);
        pin(PacketType.Play.Server.ENTITY_TELEPORT, clientboundPlay, "teleport_entity", version);
        pin(PacketType.Play.Server.OPEN_WINDOW, clientboundPlay, "open_screen", version);
        pin(PacketType.Play.Server.CLOSE_WINDOW, clientboundPlay, "container_close", version);
        pin(PacketType.Play.Server.OPEN_HORSE_WINDOW, clientboundPlay, "mount_screen_open", version);
        pin(PacketType.Play.Server.OPEN_BOOK, clientboundPlay, "open_book", version);
        pin(PacketType.Play.Server.OPEN_SIGN_EDITOR, clientboundPlay, "open_sign_editor", version);
        pin(PacketType.Play.Server.DEATH_COMBAT_EVENT, clientboundPlay, "player_combat_kill", version);
        pin(PacketType.Play.Server.RESOURCE_PACK_SEND, clientboundPlay, "resource_pack_push", version);
        pin(PacketType.Play.Server.CONFIGURATION_START, clientboundPlay, "start_configuration", version);
        pin(PacketType.Play.Server.SHOW_DIALOG, clientboundPlay, "show_dialog", version);
        pin(PacketType.Play.Server.CLEAR_DIALOG, clientboundPlay, "clear_dialog", version);
        pin(PacketType.Play.Server.CHUNK_DATA, clientboundPlay, "level_chunk_with_light", version);
        pin(PacketType.Play.Server.UPDATE_LIGHT, clientboundPlay, "light_update", version);
        pin(PacketType.Play.Server.CHUNK_BIOMES, clientboundPlay, "chunks_biomes", version);

        pin(PacketType.Configuration.Client.PLUGIN_MESSAGE, serverboundConfig, "custom_payload", version);
        pin(PacketType.Configuration.Client.CONFIGURATION_END_ACK, serverboundConfig, "finish_configuration", version);
        pin(PacketType.Configuration.Server.REGISTRY_DATA, clientboundConfig, "registry_data", version);
        pin(PacketType.Configuration.Server.CONFIGURATION_END, clientboundConfig, "finish_configuration", version);

        if (!failures.isEmpty()) {
            failures.forEach(System.err::println);
            System.exit(1);
        }
        System.out.println("PacketEvents agrees with Foton on every packet id of protocol " + packets.get("version"));
    }

    private static void permutation(String label, PacketTypeCommon[] types, JsonArray vanilla, ClientVersion version) {
        boolean[] claimed = new boolean[vanilla.size()];
        int assigned = 0;
        for (PacketTypeCommon type : types) {
            int id = type.getId(version);
            if (id < 0) continue;
            assigned++;
            if (id >= vanilla.size()) {
                failures.add(label + ": " + type.getName() + " has id " + id + ", Foton has " + vanilla.size() + " packets");
            } else if (claimed[id]) {
                failures.add(label + ": " + type.getName() + " shares id " + id + " (" + vanilla.get(id).getAsString() + ")");
            } else {
                claimed[id] = true;
            }
        }
        if (assigned != vanilla.size()) {
            failures.add(label + ": PacketEvents assigns " + assigned + " ids, Foton has " + vanilla.size() + " packets");
        }
    }

    private static void pin(PacketTypeCommon type, JsonArray vanilla, String name, ClientVersion version) {
        int id = type.getId(version);
        String actual = id >= 0 && id < vanilla.size() ? vanilla.get(id).getAsString() : "none";
        if (!actual.equals(name)) {
            failures.add(type.getName() + " is " + actual + " (id " + id + ") in Foton, expected " + name);
        }
    }
}
