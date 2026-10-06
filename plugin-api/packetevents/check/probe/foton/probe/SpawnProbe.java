package foton.probe;

import java.util.List;
import java.util.Map;
import org.bukkit.Location;
import org.bukkit.command.Command;
import org.bukkit.command.CommandExecutor;
import org.bukkit.command.CommandSender;
import org.bukkit.entity.Entity;
import org.bukkit.entity.Player;

/** `/probespawn`: spawns each entity class Zelda Civ spawns through
 * `World.spawn`, and writes down whether it came out alive and typed. */
final class SpawnProbe implements CommandExecutor {
    private static final List<Class<? extends Entity>> CLASSES = List.of(
        org.bukkit.entity.Villager.class, org.bukkit.entity.Allay.class,
        org.bukkit.entity.TextDisplay.class, org.bukkit.entity.ItemDisplay.class,
        org.bukkit.entity.BlockDisplay.class, org.bukkit.entity.Interaction.class,
        org.bukkit.entity.ExperienceOrb.class);

    private final Map<String, String> facts;

    SpawnProbe(Map<String, String> facts) {
        this.facts = facts;
    }

    @Override
    public boolean onCommand(CommandSender sender, Command command, String label, String[] args) {
        if (!(sender instanceof Player player)) return true;
        Location at = player.getLocation().add(2, 0, 0);
        for (Class<? extends Entity> type : CLASSES) {
            String name = type.getSimpleName();
            try {
                boolean[] configured = {false};
                Entity spawned = player.getWorld().spawn(at, type, entity -> configured[0] = true);
                facts.put("spawn " + name, spawned.getType() + " valid " + spawned.isValid()
                    + " typed " + type.isInstance(spawned) + " configured " + configured[0]);
            } catch (Throwable error) {
                facts.put("spawn " + name, "threw " + error);
            }
        }
        return true;
    }
}
