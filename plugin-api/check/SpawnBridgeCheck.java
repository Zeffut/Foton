import org.bukkit.event.EventPriority;
import org.bukkit.event.Listener;
import org.bukkit.event.entity.CreatureSpawnEvent;
import com.destroystokyo.paper.event.entity.PreCreatureSpawnEvent;

/** Invoked by the Rust JNI test so the real forwarding descriptors are exercised. */
public final class SpawnBridgeCheck {
    public static int preCalls;
    public static int spawnCalls;

    public static void install() {
        var owner = new org.bukkit.plugin.java.JavaPlugin() {};
        owner.setEnabled(true);
        Listener listener = new Listener() {};
        foton.EventBridge.register(listener, PreCreatureSpawnEvent.class, EventPriority.NORMAL,
            (ignored, event) -> {
                var spawn = (PreCreatureSpawnEvent) event;
                if (spawn.getReason() == CreatureSpawnEvent.SpawnReason.TRIAL_SPAWNER) preCalls++;
                spawn.setCancelled(true);
            }, owner);
        foton.EventBridge.register(listener, CreatureSpawnEvent.class, EventPriority.NORMAL,
            (ignored, event) -> {
                var spawn = (CreatureSpawnEvent) event;
                if (spawn.getSpawnReason() == CreatureSpawnEvent.SpawnReason.BEEHIVE) spawnCalls++;
                spawn.setCancelled(true);
            }, owner);
    }

    public static boolean absentEntityIsDefault() {
        return new foton.FotonEntity(java.util.UUID.randomUUID()).getEntitySpawnReason()
            == CreatureSpawnEvent.SpawnReason.DEFAULT;
    }
}
