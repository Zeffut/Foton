import org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason;

/** Compiled against the Paper ABI fixture, executed with only Foton's API. */
public final class PaperSpawnReasonConsumer {
    public static void main(String[] args) {
        if (!SpawnReason.BEEHIVE.name().equals("BEEHIVE")
                || !SpawnReason.TRIAL_SPAWNER.name().equals("TRIAL_SPAWNER")) {
            throw new AssertionError("Paper spawn-reason field linkage");
        }
    }
}
