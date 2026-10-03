import java.util.Set;
import org.bukkit.Bukkit;
import org.bukkit.Location;
import org.bukkit.World;
import org.bukkit.entity.Entity;
import org.bukkit.entity.EntityType;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.plugin.java.JavaPlugin;

/** End-to-end JNI probe for an actual world entity, loaded as a Bukkit plugin. */
public final class EntityScoreboardTagsProbe extends JavaPlugin implements Listener {
    private Entity entity;

    @Override public void onEnable() {
        getServer().getPluginManager().registerEvents(this, this);
        getServer().getScheduler().runTaskLater(this, this::checkEntityTags, 200L);
    }

    private void checkEntityTags() {
        World world = Bukkit.getWorlds().getFirst();
        Location spawn = world.getSpawnLocation();
        entity = world.spawnEntity(spawn, EntityType.ARMOR_STAND);
        if (entity == null) throw new AssertionError("entity spawn failed");

        Set<String> tags = entity.getScoreboardTags();
        if (!tags.isEmpty()) throw new AssertionError("fresh entity has scoreboard tags");
        if (!entity.addScoreboardTag("foton:probe")) throw new AssertionError("tag was not added");
        if (entity.addScoreboardTag("foton:probe")) throw new AssertionError("duplicate tag was added");
        if (!tags.contains("foton:probe")) throw new AssertionError("live set missed explicit addition");
        if (!entity.removeScoreboardTag("foton:probe")) throw new AssertionError("tag was not removed");
        if (entity.removeScoreboardTag("foton:probe")) throw new AssertionError("missing tag was removed");
        if (!tags.isEmpty()) throw new AssertionError("live set missed explicit removal");

        if (!tags.add("foton:direct")) throw new AssertionError("set addition failed");
        if (!entity.getScoreboardTags().contains("foton:direct")) throw new AssertionError("set addition did not persist");
        var iterator = tags.iterator();
        if (!iterator.hasNext() || !iterator.next().equals("foton:direct"))
            throw new AssertionError("set iterator missed tag");
        iterator.remove();
        if (!tags.isEmpty()) throw new AssertionError("iterator removal did not persist");
        tags.add("foton:clear");
        tags.clear();
        if (!entity.getScoreboardTags().isEmpty()) throw new AssertionError("clear did not persist");
        getLogger().info("ENTITY_TAGS_PROBE_OK");
    }

    @EventHandler public void onJoin(PlayerJoinEvent event) {
        Set<String> tags = event.getPlayer().getScoreboardTags();
        if (tags.contains("foton:player")) {
            if (!event.getPlayer().removeScoreboardTag("foton:player"))
                throw new AssertionError("restored player tag was not removed");
            if (!tags.isEmpty()) throw new AssertionError("player live set missed restored-tag removal");
            getLogger().info("PLAYER_TAGS_RESTORED_OK");
            return;
        }
        if (!tags.add("foton:player")) throw new AssertionError("player tag was not added");
        if (!event.getPlayer().getScoreboardTags().contains("foton:player"))
            throw new AssertionError("player tag is absent from its native entity");
        getLogger().info("PLAYER_TAGS_SAVED_OK");
    }

    @Override public void onDisable() {
        if (entity != null) entity.remove();
    }
}
