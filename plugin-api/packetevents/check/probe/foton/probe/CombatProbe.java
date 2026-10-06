package foton.probe;

import java.util.Map;
import org.bukkit.entity.AbstractHorse;
import org.bukkit.entity.Entity;
import org.bukkit.entity.Item;
import org.bukkit.entity.LivingEntity;
import org.bukkit.entity.Projectile;
import org.bukkit.event.EventHandler;
import org.bukkit.event.EventPriority;
import org.bukkit.event.Listener;
import org.bukkit.event.entity.EntityCombustByBlockEvent;
import org.bukkit.event.entity.EntityCombustByEntityEvent;
import org.bukkit.event.entity.EntityCombustEvent;
import org.bukkit.event.entity.EntityDamageByBlockEvent;
import org.bukkit.event.entity.EntityDamageByEntityEvent;
import org.bukkit.event.entity.EntityDamageEvent;
import org.bukkit.event.entity.EntityDeathEvent;
import org.bukkit.plugin.java.JavaPlugin;

/** Writes down what combat and entity events look like to a plugin.
 *
 * Entities are steered by scoreboard tags a test gives them: `probe_double`
 * doubles the raw damage, `probe_cancel` refuses it, `probe_keep` keeps a
 * dropped item out of lava and fire, `probe_nofire` refuses to catch fire. The
 * first event of each shape is recorded, so a burning entity does not flood
 * the report.
 */
final class CombatProbe implements Listener {
    private final JavaPlugin plugin;
    private final Map<String, String> facts;

    CombatProbe(JavaPlugin plugin, Map<String, String> facts) {
        this.plugin = plugin;
        this.facts = facts;
    }

    @EventHandler(priority = EventPriority.HIGH)
    public void onDamage(EntityDamageEvent event) {
        Entity entity = event.getEntity();
        double raw = event.getDamage();
        double finalBefore = event.getFinalDamage();
        var tags = entity.getScoreboardTags();
        String verdict = "";
        if (tags.contains("probe_double")) {
            event.setDamage(raw * 2);
            verdict = " doubled to " + round(event.getDamage()) + " final " + round(event.getFinalDamage());
            if (entity instanceof LivingEntity living) {
                double before = living.getHealth();
                String key = "health " + label(entity) + entity.getType() + " " + event.getCause();
                plugin.getServer().getScheduler().runTaskLater(plugin, () ->
                    facts.putIfAbsent(key, round(before) + " -> " + round(living.getHealth())), 1L);
            }
        }
        if (tags.contains("probe_cancel")
                || (entity instanceof Item && tags.contains("probe_keep"))) {
            event.setCancelled(true);
            verdict = " cancelled";
            String key = "still there 2s after refusing " + label(entity) + entity.getType() + " " + event.getCause();
            plugin.getServer().getScheduler().runTaskLater(plugin, () ->
                facts.putIfAbsent(key, String.valueOf(entity.isValid())), 40L);
        }
        facts.putIfAbsent("damage " + label(entity) + entity.getType() + " " + event.getCause() + " " + event.getClass().getSimpleName(),
            "damager " + damager(event) + " raw " + round(raw) + " final " + round(finalBefore)
                + (event instanceof EntityDamageByEntityEvent byEntity && byEntity.isCritical() ? " critical" : "")
                + verdict);
    }

    /** Runs after every other listener, as Zelda's MONITOR listeners do. */
    @EventHandler(priority = EventPriority.MONITOR, ignoreCancelled = true)
    public void onDamageSeen(EntityDamageByEntityEvent event) {
        facts.putIfAbsent("monitor " + label(event.getEntity()) + event.getEntity().getType() + " " + event.getCause(),
            "raw " + round(event.getDamage()) + " final " + round(event.getFinalDamage()));
    }

    @EventHandler
    public void onDeath(EntityDeathEvent event) {
        LivingEntity entity = event.getEntity();
        facts.putIfAbsent("death " + entity.getType(), entity.getClass().getSimpleName()
            + " horse " + (entity instanceof AbstractHorse));
    }

    @EventHandler(priority = EventPriority.HIGH)
    public void onCombust(EntityCombustEvent event) {
        Entity entity = event.getEntity();
        String by = event instanceof EntityCombustByEntityEvent byEntity ? "entity " + byEntity.getCombuster().getType()
            : event instanceof EntityCombustByBlockEvent byBlock
                ? "block " + (byBlock.getCombuster() == null ? "unknown" : byBlock.getCombuster().getType())
                : "nothing";
        boolean refused = entity.getScoreboardTags().contains("probe_nofire");
        if (refused) event.setCancelled(true);
        facts.putIfAbsent("combust " + entity.getType() + " " + event.getClass().getSimpleName(),
            "by " + by + " for " + event.getDuration() + "s" + (refused ? " cancelled" : ""));
        if (refused) {
            plugin.getServer().getScheduler().runTaskLater(plugin, () ->
                facts.putIfAbsent("fire ticks after refusal " + entity.getType(), String.valueOf(entity.getFireTicks())), 2L);
        }
    }

    private static String damager(EntityDamageEvent event) {
        if (event instanceof EntityDamageByEntityEvent byEntity) {
            Entity damager = byEntity.getDamager();
            String shooter = damager instanceof Projectile projectile && projectile.getShooter() instanceof Entity source
                ? " shot by " + source.getType() : "";
            return damager.getType() + shooter;
        }
        if (event instanceof EntityDamageByBlockEvent byBlock) {
            return "block " + (byBlock.getDamager() == null ? "unknown" : byBlock.getDamager().getType());
        }
        return "none";
    }

    /** The `t_` tag a test gave the entity, which keeps its facts apart. */
    private static String label(Entity entity) {
        for (String tag : entity.getScoreboardTags()) if (tag.startsWith("t_")) return tag + " ";
        return "";
    }

    private static String round(double value) {
        return String.valueOf(Math.round(value * 100) / 100.0);
    }
}
