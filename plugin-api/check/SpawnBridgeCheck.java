import org.bukkit.event.EventPriority;
import org.bukkit.event.Listener;
import org.bukkit.event.entity.CreatureSpawnEvent;
import com.destroystokyo.paper.event.entity.PreCreatureSpawnEvent;

/** Invoked by the Rust JNI test so the real forwarding descriptors are exercised. */
public final class SpawnBridgeCheck {
    public static int preCalls;
    public static int spawnCalls;
    private static boolean cancelSpawns = true;
    private static java.util.UUID spawnedBee;

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
                spawnedBee = spawn.getEntity().getUniqueId();
                spawn.setCancelled(cancelSpawns);
            }, owner);
    }

    public static boolean absentEntityIsDefault() {
        return new foton.FotonEntity(java.util.UUID.randomUUID()).getEntitySpawnReason()
            == CreatureSpawnEvent.SpawnReason.DEFAULT;
    }

    public static void allowBeehiveRelease() {
        cancelSpawns = false;
        spawnCalls = 0;
        spawnedBee = null;
    }

    public static void assertReleasedBee(String uuid) {
        org.bukkit.entity.Entity bee = new foton.FotonEntity(java.util.UUID.fromString(uuid));
        if (spawnCalls != 1 || !bee.getUniqueId().equals(spawnedBee)) {
            throw new AssertionError("Inserted bee must be the one observed in the BEEHIVE event");
        }
        CreatureSpawnEvent.SpawnReason actual = bee.getEntitySpawnReason();
        if (actual != CreatureSpawnEvent.SpawnReason.BEEHIVE) {
            throw new AssertionError("Inserted bee: expected BEEHIVE, got " + actual);
        }
    }

    public static void assertClassSpawnContracts(String worldName) {
        var world = new foton.FotonWorld(worldName);
        var location = new org.bukkit.Location(world, 8.5, 64.0, 8.5);

        org.bukkit.entity.AbstractCow cow = world.spawn(
            location, org.bukkit.entity.AbstractCow.class);
        if (!(cow instanceof org.bukkit.entity.Cow)
                || cow.getType() != org.bukkit.entity.EntityType.COW) {
            throw new AssertionError("AbstractCow class spawn must return a Cow wrapper");
        }

        org.bukkit.entity.AbstractCubeMob cube = world.spawn(
            location, org.bukkit.entity.AbstractCubeMob.class);
        if (!(cube instanceof org.bukkit.entity.Slime)
                || cube.getType() != org.bukkit.entity.EntityType.SLIME) {
            throw new AssertionError("AbstractCubeMob class spawn must return a Slime wrapper");
        }
        cube.setSize(3);
        if (cube.getSize() != 3) {
            throw new AssertionError("AbstractCubeMob size must round-trip through native state");
        }
        cube.setSize(0);
        if (cube.getSize() != 1) {
            throw new AssertionError("AbstractCubeMob size must use the native 1..127 clamp");
        }
        cube.setSize(3);
        if (!cube.canWander()) {
            throw new AssertionError("a newly spawned cube mob must wander by default");
        }
        cube.setWander(false);
        if (cube.canWander()) {
            throw new AssertionError("setWander(false) must update native cube state");
        }
        cube.setWander(true);
        if (!cube.canWander()) {
            throw new AssertionError("setWander(true) must update native cube state");
        }

        org.bukkit.entity.SizedFireball fireball = world.spawn(
            location, org.bukkit.entity.SizedFireball.class);
        if (fireball.getType() != org.bukkit.entity.EntityType.FIREBALL
                || fireball.getDisplayItem().getType() != org.bukkit.Material.FIRE_CHARGE) {
            throw new AssertionError("SizedFireball class spawn must expose its native display item");
        }
        fireball.setDisplayItem(new org.bukkit.inventory.ItemStack(org.bukkit.Material.STONE, 4));
        if (fireball.getDisplayItem().getType() != org.bukkit.Material.STONE
                || fireball.getDisplayItem().getAmount() != 1) {
            throw new AssertionError("SizedFireball display item must round-trip and clamp to one");
        }

        org.bukkit.entity.TippedArrow tipped = world.spawn(
            location, org.bukkit.entity.TippedArrow.class);
        if (!(tipped instanceof foton.FotonTippedArrow)
                || tipped.getType() != org.bukkit.entity.EntityType.ARROW
                || tipped.getBasePotionType() != org.bukkit.potion.PotionType.WATER) {
            throw new AssertionError("TippedArrow class spawn must return WATER before control returns");
        }

        org.bukkit.entity.Arrow ordinary = world.spawn(location, org.bukkit.entity.Arrow.class);
        if (!(ordinary instanceof foton.FotonArrow)
                || ordinary instanceof org.bukkit.entity.TippedArrow
                || ordinary.getBasePotionType() != null) {
            throw new AssertionError("ordinary Arrow identity and potion state must remain ordinary");
        }
    }
}
