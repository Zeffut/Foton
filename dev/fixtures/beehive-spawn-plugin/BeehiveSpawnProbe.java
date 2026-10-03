package foton.fixture.beehive;

import java.util.concurrent.atomic.AtomicBoolean;
import org.bukkit.command.PluginCommand;
import org.bukkit.entity.Bee;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.event.entity.CreatureSpawnEvent;
import org.bukkit.plugin.java.JavaPlugin;

/** Proves a Paper-compiled listener sees the live pending bee, not a generic wrapper. */
public final class BeehiveSpawnProbe extends JavaPlugin implements Listener {
    private final AtomicBoolean releaseAllowed = new AtomicBoolean();
    private final AtomicBoolean cancellationReported = new AtomicBoolean();
    private final AtomicBoolean releaseReported = new AtomicBoolean();

    @Override public void onEnable() {
        getServer().getPluginManager().registerEvents(this, this);
        PluginCommand command = getCommand("allowbeerelease");
        if (command == null) {
            throw new IllegalStateException("fixture command was not registered");
        }
        command.setExecutor((sender, ignored, label, args) -> {
            releaseAllowed.set(true);
            System.out.println("[beehive-probe] BEEHIVE_RELEASE_ALLOWED");
            return true;
        });
        System.out.println("[beehive-probe] enabled");
    }

    @EventHandler public void onCreatureSpawn(CreatureSpawnEvent event) {
        if (event.getSpawnReason() != CreatureSpawnEvent.SpawnReason.BEEHIVE) {
            return;
        }
        if (!(event.getEntity() instanceof Bee bee)) {
            System.out.println("[beehive-probe] WRONG_ENTITY_TYPE " + event.getEntity().getClass());
            event.setCancelled(true);
            return;
        }
        if (bee.getWorld() == null) {
            System.out.println("[beehive-probe] PENDING_BEE_NOT_RESOLVED");
            event.setCancelled(true);
            return;
        }
        if (!releaseAllowed.get()) {
            event.setCancelled(true);
            if (cancellationReported.compareAndSet(false, true)) {
                System.out.println("[beehive-probe] BEEHIVE_TYPED_PENDING_CANCELLED");
            }
        } else if (releaseReported.compareAndSet(false, true)) {
            System.out.println("[beehive-probe] BEEHIVE_TYPED_PENDING_ALLOWED");
        }
    }
}
