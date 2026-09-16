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

    public static void assertPrePublicationArrowPotionNoOp() {
        foton.Native.setArrowPotion(
            "00000000-0000-0000-0000-000000000099", "minecraft:water");
    }

    public static void assertPrePublicationLiveAnimalBreedItems(String uuid) {
        org.bukkit.entity.Animals animal = new foton.FotonCow(java.util.UUID.fromString(uuid));
        if (animal.isBreedItem(org.bukkit.Material.WHEAT)
                || animal.isBreedItem(new org.bukkit.inventory.ItemStack(org.bukkit.Material.WHEAT))) {
            throw new AssertionError("an unpublished item registry must safely reject breed items");
        }
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
        if (!(cow instanceof org.bukkit.entity.Cow liveCow)
                || cow.getType() != org.bukkit.entity.EntityType.COW) {
            throw new AssertionError("AbstractCow class spawn must return a Cow wrapper");
        }
        liveCow.setVariant(org.bukkit.entity.Cow.Variant.COLD);
        liveCow.setSoundVariant(org.bukkit.entity.Cow.SoundVariant.MOODY);
        if (liveCow.getVariant() != org.bukkit.entity.Cow.Variant.COLD
                || liveCow.getSoundVariant() != org.bukkit.entity.Cow.SoundVariant.MOODY) {
            throw new AssertionError("Cow variant and sound variant must round-trip native state");
        }
        java.util.UUID cause = java.util.UUID.fromString(
            "00000000-0000-0000-0000-000000000042");
        cow.setBreedCause(cause);
        expectIllegalArgument(() -> cow.setLoveModeTicks(-1));
        if (cow.getLoveModeTicks() != 0) {
            throw new AssertionError("negative love ticks must be rejected before JNI mutation");
        }
        cow.setLoveModeTicks(0);
        if (cow.isLoveMode() || cow.getLoveModeTicks() != 0) {
            throw new AssertionError("zero love ticks must leave love mode disabled");
        }
        cow.setLoveModeTicks(42);
        if (!cause.equals(cow.getBreedCause()) || !cow.isLoveMode()
                || cow.getLoveModeTicks() != 42 || !cow.canBreed()) {
            throw new AssertionError("Animals breeding state must round-trip through native state");
        }
        if (!cow.isBreedItem(org.bukkit.Material.WHEAT)
                || cow.isBreedItem(org.bukkit.Material.STONE)
                || !cow.isBreedItem(new org.bukkit.inventory.ItemStack(org.bukkit.Material.WHEAT))) {
            throw new AssertionError("Animals breed-item checks must use the native food predicate");
        }
        cow.setAge(17);
        if (cow.canBreed() || cow.getLoveModeTicks() != 42) {
            throw new AssertionError("positive age cooldown must disable breeding independently of love");
        }
        cow.setBreed(false);
        if (cow.getAge() != 6000 || cow.canBreed()
                || cow.getLoveModeTicks() != 42 || !cause.equals(cow.getBreedCause())) {
            throw new AssertionError("adult setBreed(false) must set only the 6000-tick age cooldown");
        }
        cow.setBreed(true);
        if (cow.getAge() != 0 || !cow.canBreed()
                || cow.getLoveModeTicks() != 42 || !cause.equals(cow.getBreedCause())) {
            throw new AssertionError("setBreed(true) must clear age cooldown independently of love");
        }
        cow.setAge(-123);
        cow.setBreed(false);
        if (cow.getAge() != -123 || cow.canBreed() || cow.getLoveModeTicks() != 42) {
            throw new AssertionError("setBreed(false) must not change a baby or love mode");
        }
        cow.setBreed(true);
        if (cow.getAge() != 0 || !cow.canBreed() || cow.getLoveModeTicks() != 42) {
            throw new AssertionError("setBreed(true) must instantly mature a baby without changing love");
        }
        cow.setBreedCause(null);
        cow.setLoveModeTicks(0);
        if (cow.getBreedCause() != null || cow.isLoveMode() || cow.getLoveModeTicks() != 0) {
            throw new AssertionError("Animals state clearing must update the native animal");
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
        ((org.bukkit.entity.LivingEntity) cube).setHealth(0.0);
        cube.setSize(5);
        if (((org.bukkit.entity.LivingEntity) cube).getHealth() != 0.0) {
            throw new AssertionError("resizing a dead cube mob must not heal it");
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

        int entityCount = world.getEntities().size();
        expectIllegalArgument(() -> world.spawn(location, (Class) null));
        expectIllegalArgument(() -> world.spawn(location, org.bukkit.entity.Entity.class));
        expectIllegalArgument(() -> world.spawn(location, org.bukkit.entity.Egg.class));
        if (world.getEntities().size() != entityCount) {
            throw new AssertionError("failed class spawns must not leave native orphan entities");
        }

        String malformed = foton.Native.spawnEntity(
            worldName, location.getX(), location.getY(), location.getZ(), "arrow", "minecraft:not_a_potion");
        if (malformed != null || world.getEntities().size() != entityCount) {
            throw new AssertionError("malformed initialized spawns must fail before publication");
        }
    }

    private static void expectIllegalArgument(Runnable action) {
        try {
            action.run();
        } catch (IllegalArgumentException expected) {
            return;
        }
        throw new AssertionError("unsupported class spawn must throw IllegalArgumentException");
    }
}
