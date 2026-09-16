import java.util.Arrays;
import org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason;

/** Paper 26.2's enum ABI and the server-name fallback used by the bridge. */
public final class SpawnReasonCheck {
    public static void main(String[] args) throws Exception {
        if (args.length != 0 && args[0].equals("fallback")) {
            var parse = foton.EventBridge.class.getDeclaredMethod("spawnReason", String.class);
            parse.setAccessible(true);
            for (String reason : new String[] { null, "", "future_server_reason" }) {
                if (parse.invoke(null, reason) != SpawnReason.DEFAULT) {
                    throw new AssertionError("absent/unknown server reason must be DEFAULT: " + reason);
                }
            }
            return;
        }
        String[] expected = ("NATURAL JOCKEY CHUNK_GEN SPAWNER TRIAL_SPAWNER EGG SPAWNER_EGG "
            + "LIGHTNING BUILD_SNOWMAN BUILD_IRONGOLEM BUILD_COPPERGOLEM BUILD_WITHER "
            + "VILLAGE_DEFENSE VILLAGE_INVASION BREEDING SLIME_SPLIT REINFORCEMENTS NETHER_PORTAL "
            + "DISPENSE_EGG INFECTION CURED OCELOT_BABY SILVERFISH_BLOCK MOUNT TRAP ENDER_PEARL "
            + "SHOULDER_ENTITY DROWNED SHEARED EXPLOSION RAID PATROL BEEHIVE PIGLIN_ZOMBIFIED "
            + "SPELL FROZEN METAMORPHOSIS DUPLICATION COMMAND ENCHANTMENT OMINOUS_ITEM_SPAWNER "
            + "BUCKET POTION_EFFECT REANIMATE REHYDRATION CUSTOM DEFAULT").split(" ");
        String[] actual = Arrays.stream(SpawnReason.values()).map(Enum::name).toArray(String[]::new);
        if (!Arrays.equals(actual, expected)) {
            throw new AssertionError("Paper 26.2 SpawnReason ABI: " + Arrays.toString(actual));
        }
        Deprecated deprecated = SpawnReason.class.getField("CHUNK_GEN").getAnnotation(Deprecated.class);
        if (deprecated == null || !deprecated.forRemoval() || !deprecated.since().equals("1.14")) {
            throw new AssertionError("CHUNK_GEN must retain Paper's deprecation metadata");
        }
    }
}
