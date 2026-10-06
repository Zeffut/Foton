package foton.probe;

import java.util.Map;
import org.bukkit.Chunk;
import org.bukkit.Location;
import org.bukkit.NamespacedKey;
import org.bukkit.World;
import org.bukkit.entity.Entity;
import org.bukkit.entity.EntityType;
import org.bukkit.entity.Player;
import org.bukkit.persistence.PersistentDataContainer;
import org.bukkit.persistence.PersistentDataType;
import org.bukkit.plugin.java.JavaPlugin;

/** `/probepdc write` stores persistent data on the player, on two entities and
 * on the chunk; `/probepdc read` -- in a later run on the same world -- writes
 * down what is still there. Each value is also read back through a second
 * handle, since the data must live with the owner and not with one Java object.
 */
final class PdcProbe {
    private static final NamespacedKey TEXT = new NamespacedKey("probe", "text");
    private static final NamespacedKey COUNT = new NamespacedKey("probe", "count");
    private static final NamespacedKey NESTED = new NamespacedKey("probe", "nested");
    private static final NamespacedKey GONE = new NamespacedKey("probe", "gone");
    private static final String STAND_TAG = "probepdc_stand";
    private static final String PIG_TAG = "probepdc_pig";

    private PdcProbe() {}

    static void run(JavaPlugin plugin, Player player, String mode, Map<String, String> facts) {
        if (mode.equals("write")) write(player, facts);
        else read(player, facts);
    }

    private static void store(PersistentDataContainer pdc, String text, int count) {
        pdc.set(TEXT, PersistentDataType.STRING, text);
        pdc.set(COUNT, PersistentDataType.INTEGER, count);
        PersistentDataContainer inner = pdc.getAdapterContext().newPersistentDataContainer();
        inner.set(COUNT, PersistentDataType.LONG, 5_000_000_000L);
        pdc.set(NESTED, PersistentDataType.TAG_CONTAINER, inner);
        pdc.set(GONE, PersistentDataType.BYTE, (byte) 1);
        pdc.remove(GONE);
    }

    private static String describe(PersistentDataContainer pdc) {
        PersistentDataContainer inner = pdc.get(NESTED, PersistentDataType.TAG_CONTAINER);
        Long nested = inner == null ? null : inner.get(COUNT, PersistentDataType.LONG);
        return "text=" + pdc.get(TEXT, PersistentDataType.STRING)
            + " count=" + pdc.get(COUNT, PersistentDataType.INTEGER)
            + " nested=" + nested
            + " gone=" + pdc.has(GONE)
            + " size=" + pdc.getKeys().size();
    }

    private static void write(Player player, Map<String, String> facts) {
        Location here = player.getLocation();
        World world = here.getWorld();

        store(player.getPersistentDataContainer(), "player-value", 11);
        facts.put("write player via second handle",
            describe(org.bukkit.Bukkit.getPlayer(player.getUniqueId()).getPersistentDataContainer()));

        Entity stand = world.spawnEntity(here, EntityType.ARMOR_STAND);
        stand.addScoreboardTag(STAND_TAG);
        stand.setPersistent(true);
        store(stand.getPersistentDataContainer(), "stand-value", 22);
        facts.put("write stand via second handle",
            describe(org.bukkit.Bukkit.getEntity(stand.getUniqueId()).getPersistentDataContainer()));

        Entity pig = world.spawnEntity(here, EntityType.PIG);
        pig.addScoreboardTag(PIG_TAG);
        pig.setPersistent(true);
        store(pig.getPersistentDataContainer(), "pig-value", 33);

        Chunk chunk = here.getChunk();
        store(chunk.getPersistentDataContainer(), "chunk-value", 44);
        facts.put("write chunk via second handle",
            describe(world.getChunkAt(chunk.getX(), chunk.getZ()).getPersistentDataContainer()));
        facts.put("write chunk", chunk.getX() + "," + chunk.getZ());
    }

    private static void read(Player player, Map<String, String> facts) {
        Location here = player.getLocation();
        World world = here.getWorld();
        facts.put("read player", describe(player.getPersistentDataContainer()));
        int stands = 0;
        int pigs = 0;
        for (Entity entity : world.getEntities()) {
            if (entity.getScoreboardTags().contains(STAND_TAG)) {
                facts.put("read stand " + stands++, entity.getType() + " " + describe(entity.getPersistentDataContainer()));
            }
            if (entity.getScoreboardTags().contains(PIG_TAG)) {
                facts.put("read pig " + pigs++, entity.getType() + " " + describe(entity.getPersistentDataContainer()));
            }
        }
        facts.put("read entities found", "stands " + stands + " pigs " + pigs);
        Chunk chunk = here.getChunk();
        facts.put("read chunk " + chunk.getX() + "," + chunk.getZ(), describe(chunk.getPersistentDataContainer()));
    }
}
