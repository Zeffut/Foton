package foton;

import org.bukkit.Color;
import org.bukkit.Location;
import org.bukkit.Particle;
import org.bukkit.Vibration;

/** Turns a particle's Bukkit data into the payload Foton encodes on the wire.
 *
 * <p>The checks are Paper's: data of the wrong type is an
 * {@code IllegalArgumentException} at the call, not a malformed packet a
 * client disconnects over. */
public final class FotonParticles {
    private FotonParticles() { }

    /** {@code World.spawnParticle}: to {@code receivers} (everyone in the world
     * when null) who can see {@code source}. */
    public static void spawn(org.bukkit.World world, Particle particle, java.util.List<org.bukkit.entity.Player> receivers,
            org.bukkit.entity.Player source, double x, double y, double z, int count,
            double offsetX, double offsetY, double offsetZ, double extra, Object data, boolean force) {
        String payload = encode(particle, data);
        String[] targets = null;
        if (receivers != null || source != null) {
            java.util.ArrayList<String> ids = new java.util.ArrayList<>();
            for (org.bukkit.entity.Player receiver : receivers != null ? receivers : world.getPlayers())
                if (receiver != null && (source == null || receiver.canSee(source)))
                    ids.add(receiver.getUniqueId().toString());
            targets = ids.toArray(new String[0]);
        }
        Native.spawnParticles(world.getName(), targets, particle.getKey().toString(), x, y, z, count,
            offsetX, offsetY, offsetZ, extra, payload, force);
    }

    /** {@code Player.spawnParticle}: to that player alone. */
    public static void spawn(org.bukkit.entity.Player player, Particle particle, double x, double y, double z, int count,
            double offsetX, double offsetY, double offsetZ, double extra, Object data, boolean force) {
        Native.playerParticles(player.getUniqueId().toString(), particle.getKey().toString(), x, y, z, count,
            offsetX, offsetY, offsetZ, extra, encode(particle, data), force);
    }

    /** The payload as {@code kind:field:...}, or {@code ""} for a particle that takes none. */
    static String encode(Particle particle, Object data) {
        if (particle == null) throw new IllegalArgumentException("particle");
        if (particle.isUnspawnable())
            throw new IllegalArgumentException(particle + " carries a payload no Bukkit data type expresses");
        Class<?> type = particle.getDataType();
        if (type == Void.class) {
            if (data != null)
                throw new IllegalArgumentException("data (" + data.getClass() + ") should be " + type);
            return "";
        }
        if (data == null) throw new IllegalArgumentException(particle + " requires data of type " + type);
        if (!type.isInstance(data))
            throw new IllegalArgumentException("data (" + data.getClass() + ") should be " + type);
        if (data instanceof Particle.DustTransition transition)
            return "dust_transition:" + transition.getColor().asRGB() + ":" + transition.getToColor().asRGB()
                + ":" + transition.getSize();
        if (data instanceof Particle.DustOptions dust)
            return "dust:" + dust.getColor().asRGB() + ":" + dust.getSize();
        if (data instanceof Particle.Spell spell)
            return "spell:" + spell.getColor().asRGB() + ":" + spell.getPower();
        if (data instanceof Particle.Trail trail) {
            Location target = trail.getTarget();
            return "trail:" + target.getX() + ":" + target.getY() + ":" + target.getZ() + ":"
                + trail.getColor().asRGB() + ":" + trail.getDuration();
        }
        if (data instanceof Color color) return "color:" + color.asARGB();
        if (data instanceof org.bukkit.block.data.BlockData block) return "block:" + block.getAsString();
        if (data instanceof org.bukkit.inventory.ItemStack item) return "item:" + FotonInventory.encode(item);
        if (data instanceof Float value) return "float:" + value;
        if (data instanceof Integer value) return "int:" + value;
        if (data instanceof Vibration vibration) {
            if (vibration.getDestination() instanceof Vibration.Destination.EntityDestination entity)
                return "vibration:entity:" + entity.getEntity().getUniqueId() + ":" + vibration.getArrivalTime();
            if (vibration.getDestination() instanceof Vibration.Destination.BlockDestination block) {
                Location at = block.getLocation();
                return "vibration:block:" + at.getBlockX() + ":" + at.getBlockY() + ":" + at.getBlockZ()
                    + ":" + vibration.getArrivalTime();
            }
        }
        throw new IllegalArgumentException("unsupported particle data " + data.getClass());
    }
}
