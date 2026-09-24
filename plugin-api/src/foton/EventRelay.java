package foton;

import java.util.UUID;
import org.bukkit.Location;
import org.bukkit.World;
import org.bukkit.entity.Player;
import org.bukkit.util.Vector;

/** Paper's events that Foton learned to carry after the first set.
 *
 * Every method has the same shape, so the Rust side writes the crossing once
 * (`foton-plugin/src/relay.rs`): the facts arrive as strings, the event is
 * built and dispatched through {@link EventBridge#dispatch}, and the outcome
 * goes back as one string whose fields are separated by {@link #FIELD}, with
 * booleans written `1` or `0`.
 */
public final class EventRelay {
    /** Separates the fields of an answer. */
    static final String FIELD = "\u001f";

    private EventRelay() {}

    static String bit(boolean value) {
        return value ? "1" : "0";
    }

    static String answer(Object... fields) {
        StringBuilder out = new StringBuilder();
        for (int index = 0; index < fields.length; index++) {
            if (index > 0) out.append(FIELD);
            Object field = fields[index];
            out.append(field instanceof Boolean flag ? bit(flag) : String.valueOf(field));
        }
        return out.toString();
    }

    static Player player(String uuid) {
        UUID parsed = Native.parse(uuid);
        return parsed == null ? null : new FotonPlayer(parsed);
    }

    static boolean flag(String value) {
        return "1".equals(value);
    }

    /** `x y z [yaw pitch]` in a world, as the relay writes a location. */
    static Location location(World world, String encoded) {
        String[] parts = encoded.split(" ");
        double x = Double.parseDouble(parts[0]);
        double y = Double.parseDouble(parts[1]);
        double z = Double.parseDouble(parts[2]);
        if (parts.length < 5) return new Location(world, x, y, z);
        return new Location(world, x, y, z, Float.parseFloat(parts[3]), Float.parseFloat(parts[4]));
    }

    static Vector vector(String encoded) {
        String[] parts = encoded.split(" ");
        return new Vector(Double.parseDouble(parts[0]), Double.parseDouble(parts[1]),
            Double.parseDouble(parts[2]));
    }

    static String encode(Vector vector) {
        return vector.getX() + " " + vector.getY() + " " + vector.getZ();
    }

    /** Answers `allowed, logWarning`. */
    public static String fireFailMove(String uuid, String world, String reason, String from,
            String to, String logWarning) {
        World in = new FotonWorld(world);
        io.papermc.paper.event.player.PlayerFailMoveEvent event =
            new io.papermc.paper.event.player.PlayerFailMoveEvent(player(uuid),
                io.papermc.paper.event.player.PlayerFailMoveEvent.FailReason.valueOf(reason),
                false, flag(logWarning), location(in, from), location(in, to));
        EventBridge.dispatch(event);
        return answer(event.isAllowed(), event.getLogWarning());
    }

    /** Answers `cancelled`. */
    public static String fireToggleFlight(String uuid, String flying) {
        org.bukkit.event.player.PlayerToggleFlightEvent event =
            new org.bukkit.event.player.PlayerToggleFlightEvent(player(uuid), flag(flying));
        EventBridge.dispatch(event);
        return answer(event.isCancelled());
    }

    /** Answers `cancelled, velocity`. */
    public static String fireVelocity(String uuid, String velocity) {
        org.bukkit.event.player.PlayerVelocityEvent event =
            new org.bukkit.event.player.PlayerVelocityEvent(player(uuid), vector(velocity));
        EventBridge.dispatch(event);
        return answer(event.isCancelled(), encode(event.getVelocity()));
    }
}
