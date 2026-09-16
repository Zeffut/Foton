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
    private static org.bukkit.entity.TippedArrow configuredArrow;
    private static java.util.UUID rolledBackCustomSourceArrow;
    private static java.util.UUID failedPublishCustomSourceArrow;

    public static org.bukkit.projectiles.ProjectileSource customSource() {
        return new org.bukkit.projectiles.ProjectileSource() { };
    }

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
        if (!org.bukkit.potion.PotionEffectType.INSTANT_HEALTH.isInstant()
                || !org.bukkit.potion.PotionEffectType.INSTANT_DAMAGE.isInstant()
                || !org.bukkit.potion.PotionEffectType.SATURATION.isInstant()
                || org.bukkit.potion.PotionEffectType.LUCK.isInstant()) {
            throw new AssertionError("PotionEffectType instantaneous state must use native registry behavior");
        }

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

        int beforeCallback = world.getEntities().size();
        org.bukkit.entity.TippedArrow configured = world.spawn(
            location, org.bukkit.entity.TippedArrow.class, pending -> {
                if (world.getEntities().size() != beforeCallback) {
                    throw new AssertionError("spawn callback must run before world publication");
                }
                if (!(pending instanceof foton.FotonTippedArrow)
                        || pending.getBasePotionType() != org.bukkit.potion.PotionType.WATER) {
                    throw new AssertionError("callback must receive the live initialized wrapper");
                }
                pending.setCritical(true);
                pending.setDamage(7.25);
                pending.setColor(org.bukkit.Color.fromARGB(0x7f123456));
                pending.setShooter((org.bukkit.projectiles.ProjectileSource) cow, false);
                if (!cow.equals(pending.getShooter())) {
                    throw new AssertionError("pending Arrow owner lookup must use the live entity");
                }
            });
        configuredArrow = configured;
        if (world.getEntities().size() != beforeCallback + 1
                || !configured.isCritical() || configured.getDamage() != 7.25
                || configured.getColor() == null
                || configured.getColor().asARGB() != 0x7f123456) {
            throw new AssertionError("successful callback must publish its one live mutated entity");
        }

        int beforeFailure = world.getEntities().size();
        java.util.concurrent.atomic.AtomicReference<java.util.UUID> failedPending =
            new java.util.concurrent.atomic.AtomicReference<>();
        RuntimeException marker = new RuntimeException("spawn callback marker");
        try {
            world.spawn(location, org.bukkit.entity.TippedArrow.class, pending -> {
                failedPending.set(pending.getUniqueId());
                pending.setCritical(true);
                throw marker;
            });
            throw new AssertionError("throwing callback must propagate");
        } catch (RuntimeException actual) {
            if (actual != marker) throw actual;
        }
        if (world.getEntities().size() != beforeFailure) {
            throw new AssertionError("throwing callback must publish no orphan");
        }
        if (failedPending.get() == null
                || foton.Native.entityType(failedPending.get().toString()) != null) {
            throw new AssertionError("throwing callback must remove its pending native entity");
        }

        configured.setBasePotionData(new org.bukkit.potion.PotionData(
            org.bukkit.potion.PotionType.SWIFTNESS, false, false));
        if (configured.getBasePotionData() == null
                || configured.getBasePotionData().getType() != org.bukkit.potion.PotionType.SWIFTNESS) {
            throw new AssertionError("deprecated potion-data bridge must mutate native potion state");
        }
        configured.setColor(null);
        if (configured.getColor() != null) {
            throw new AssertionError("null Arrow color must override the base-potion display color");
        }
        configured.setColor(org.bukkit.Color.fromARGB(0x7f123456));
        var luck = new org.bukkit.potion.PotionEffect(
            org.bukkit.potion.PotionEffectType.LUCK, 80, 2, true, false, true);
        if (!configured.addCustomEffect(luck, false)
                || !configured.hasCustomEffects()
                || !configured.hasCustomEffect(org.bukkit.potion.PotionEffectType.LUCK)
                || configured.getCustomEffects().size() != 1
                || !configured.removeCustomEffect(org.bukkit.potion.PotionEffectType.LUCK)
                || configured.hasCustomEffects()) {
            throw new AssertionError("Arrow custom effects must mutate live native potion contents");
        }
        var deepestLuck = new org.bukkit.potion.PotionEffect(
            org.bukkit.potion.PotionEffectType.LUCK, 600, 4, true, false, true);
        var hiddenLuck = new org.bukkit.potion.PotionEffect(
            org.bukkit.potion.PotionEffectType.LUCK, 400, 3, false, true, false, deepestLuck);
        var visibleLuck = new org.bukkit.potion.PotionEffect(
            org.bukkit.potion.PotionEffectType.LUCK, 80, 2, true, false, true, hiddenLuck);
        if (!configured.addCustomEffect(visibleLuck, false)) {
            throw new AssertionError("recursive Arrow effect must be accepted");
        }
        org.bukkit.potion.PotionEffect roundTripped = configured.getCustomEffects().get(0);
        if (roundTripped.getHiddenPotionEffect() != null
                || roundTripped.getDuration() != 80
                || roundTripped.getAmplifier() != 2
                || !roundTripped.isAmbient()
                || roundTripped.hasParticles()
                || !roundTripped.hasIcon()) {
            throw new AssertionError(
                "Paper Arrow.addCustomEffect must discard hidden effects but preserve visible details");
        }
        if (!configured.removeCustomEffect(org.bukkit.potion.PotionEffectType.LUCK)) {
            throw new AssertionError("recursive Arrow effect must remain removable");
        }
        configured.setPierceLevel(5);
        configured.setFallDistance(-3.5f);
        if (configured.getFallDistance() != -3.5f) {
            throw new AssertionError("negative Entity fall distance must round-trip exactly");
        }
        configured.setFallDistance(Float.NaN);
        if (!Float.isNaN(configured.getFallDistance())) {
            throw new AssertionError("NaN Entity fall distance must round-trip exactly");
        }
        configured.setFallDistance(4.25f);
        configured.setPickupStatus(org.bukkit.entity.AbstractArrow.PickupStatus.CREATIVE_ONLY);
        configured.setLifetimeTicks(411);
        configured.setHitSound(org.bukkit.Sound.ENTITY_ARROW_HIT_PLAYER);
        configured.setWeapon(new org.bukkit.inventory.ItemStack(org.bukkit.Material.CROSSBOW));
        configured.setShooter((org.bukkit.projectiles.ProjectileSource) cow, false);
        configured.setKnockbackStrength(-3);
        if (configured.getKnockbackStrength() != 0) {
            throw new AssertionError("Paper 26.2 knockback strength setter must remain a no-op");
        }
        configured.setKnockbackStrength(9);
        if (configured.getPierceLevel() != 5
                || configured.getFallDistance() != 4.25f
                || configured.getPickupStatus()
                    != org.bukkit.entity.AbstractArrow.PickupStatus.CREATIVE_ONLY
                || configured.getLifetimeTicks() != 411
                || configured.getHitSound() != org.bukkit.Sound.ENTITY_ARROW_HIT_PLAYER
                || configured.getWeapon() == null
                || configured.getWeapon().getType() != org.bukkit.Material.CROSSBOW
                || !configured.isShotFromCrossbow()
                || configured.getKnockbackStrength() != 0
                || configured.isInBlock()
                || !configured.getAttachedBlocks().isEmpty()
                || configured.getShooter() == null
                || !configured.getShooter().equals(cow)) {
            throw new AssertionError("AbstractArrow native state must round-trip through Paper API: "
                + "pierce=" + configured.getPierceLevel()
                + ", fallDistance=" + configured.getFallDistance()
                + ", pickup=" + configured.getPickupStatus()
                + ", lifetime=" + configured.getLifetimeTicks()
                + ", sound=" + configured.getHitSound()
                + ", weapon=" + configured.getWeapon()
                + ", crossbow=" + configured.isShotFromCrossbow()
                + ", knockback=" + configured.getKnockbackStrength()
                + ", inBlock=" + configured.isInBlock()
                + ", attached=" + configured.getAttachedBlocks()
                + ", shooter=" + configured.getShooter()
                + ", nativeShooter=" + foton.Native.entityProjectileOwner(
                    configured.getUniqueId().toString()));
        }
        java.util.List<org.bukkit.entity.Entity> nearby =
            configured.getNearbyEntities(2.0, 2.0, 2.0);
        if (nearby.stream().noneMatch(entity -> entity.equals(cow))) {
            throw new AssertionError("Entity nearby list must be a live native query");
        }
        int nearbySize = nearby.size();
        nearby.add(configured);
        if (nearby.size() != nearbySize + 1) {
            throw new AssertionError("Entity nearby result must be a mutable ArrayList copy");
        }
        org.bukkit.projectiles.ProjectileSource customSource =
            new org.bukkit.projectiles.ProjectileSource() { };
        configured.setShooter(customSource, false);
        if (configured.getShooter() != customSource) {
            throw new AssertionError("non-Entity ProjectileSource identity must round-trip");
        }
        assertConcurrentShooterSnapshots(configured, cow, customSource);
        configured.setShooter(null, false);
        if (configured.getShooter() != null
                || foton.Native.entityProjectileOwner(configured.getUniqueId().toString()) != null) {
            throw new AssertionError("clearing an Arrow shooter must clear native ownership");
        }
        configured.setShooter((org.bukkit.projectiles.ProjectileSource) cow, false);
        configured.setDamage(Double.POSITIVE_INFINITY);
        if (!Double.isInfinite(configured.getDamage()) || configured.getDamage() < 0.0) {
            throw new AssertionError("Paper accepts positive infinite AbstractArrow damage");
        }
        expectIllegalArgument(() -> configured.setDamage(Double.NaN));
        expectIllegalArgument(() -> configured.setDamage(-1.0));
        configured.setDamage(7.25);
        configured.setWeapon(new org.bukkit.inventory.ItemStack(org.bukkit.Material.AIR));
        if (configured.getWeapon() == null
                || configured.getWeapon().getType() != org.bukkit.Material.AIR) {
            throw new AssertionError("AbstractArrow AIR weapon must remain a non-null live stack");
        }
        configured.setWeapon(new org.bukkit.inventory.ItemStack(org.bukkit.Material.CROSSBOW));
        configured.setItemStack(new org.bukkit.inventory.ItemStack(org.bukkit.Material.AIR));
        if (configured.getItemStack() == null
                || configured.getItemStack().getType() != org.bukkit.Material.ARROW) {
            throw new AssertionError(
                "AbstractArrow AIR pickup item must reset to the vanilla default arrow");
        }
        configured.setItemStack(new org.bukkit.inventory.ItemStack(org.bukkit.Material.STONE, 3));
        if (configured.getItemStack() == null
                || configured.getItemStack().getType() != org.bukkit.Material.STONE
                || configured.getItemStack().getAmount() != 3) {
            throw new AssertionError("AbstractArrow pickup item must round-trip native state");
        }
        expectIllegalArgument(() -> configured.setPierceLevel(128));

        org.bukkit.entity.MushroomCow mushroom = world.spawn(
            location, org.bukkit.entity.MushroomCow.class);
        var entry = io.papermc.paper.potion.SuspiciousEffectEntry.create(
            org.bukkit.potion.PotionEffectType.LUCK, 321);
        if (!mushroom.addEffectToNextStew(entry, false)
                || !mushroom.hasEffectsForNextStew()
                || !mushroom.hasEffectForNextStew(org.bukkit.potion.PotionEffectType.LUCK)
                || mushroom.getStewEffects().size() != 1
                || mushroom.getStewEffects().get(0).duration() != 321) {
            throw new AssertionError("MushroomCow stew effects must use live native state");
        }
        var replacement = io.papermc.paper.potion.SuspiciousEffectEntry.create(
            org.bukkit.potion.PotionEffectType.LUCK, 654);
        if (mushroom.addEffectToNextStew(replacement, false)
                || !mushroom.addEffectToNextStew(replacement, true)
                || !mushroom.addEffectToNextStew(replacement, true)
                || mushroom.getStewEffects().size() != 3
                || mushroom.getStewEffects().get(0).duration() != 321
                || mushroom.getStewEffects().get(1).duration() != 654
                || mushroom.getStewEffects().get(2).duration() != 654
                || !mushroom.removeEffectFromNextStew(org.bukkit.potion.PotionEffectType.LUCK)
                || mushroom.hasEffectsForNextStew()) {
            throw new AssertionError("MushroomCow overwrite and removal must update native state");
        }
        mushroom.setStewEffects(java.util.List.of(entry, replacement));
        if (mushroom.getStewEffects().size() != 2 || !mushroom.readyToBeSheared()) {
            throw new AssertionError("MushroomCow full effect list and shear readiness must stay live");
        }
        mushroom.clearEffectsForNextStew();
        if (mushroom.hasEffectsForNextStew()) {
            throw new AssertionError("MushroomCow stew effects must clear live native state");
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

        RuntimeException rollbackMarker = new RuntimeException("custom source rollback marker");
        try {
            world.spawn(location, org.bukkit.entity.TippedArrow.class, pending -> {
                rolledBackCustomSourceArrow = pending.getUniqueId();
                pending.setShooter(new org.bukkit.projectiles.ProjectileSource() { }, false);
                throw rollbackMarker;
            });
            throw new AssertionError("throwing custom-source callback must propagate");
        } catch (RuntimeException actual) {
            if (actual != rollbackMarker) throw actual;
        }

        String unpublished = foton.Native.spawnEntityPending(
            worldName, 1_048_576.5, 64.0, 1_048_576.5, "arrow", "minecraft:water");
        if (unpublished == null) {
            throw new AssertionError("far pending Arrow must be created before publication fails");
        }
        failedPublishCustomSourceArrow = java.util.UUID.fromString(unpublished);
        var unpublishedArrow = new foton.FotonTippedArrow(failedPublishCustomSourceArrow);
        unpublishedArrow.setShooter(new org.bukkit.projectiles.ProjectileSource() { }, false);
        if (foton.Native.finishPendingSpawn(worldName, unpublished, true)) {
            throw new AssertionError("publishing into an unloaded far chunk must fail");
        }
    }

    public static void assertFailedCustomSourceSpawnsAreUnresolvable() {
        for (java.util.UUID id : java.util.List.of(
                rolledBackCustomSourceArrow, failedPublishCustomSourceArrow)) {
            String value = id.toString();
            if (foton.Native.entityType(value) != null
                    || foton.Native.entityProjectileSource(value) != null
                    || foton.Native.entityProjectileShooter(value) != null) {
                throw new AssertionError(
                    "failed pending spawn retained a resolvable ProjectileSource: " + id);
            }
        }
    }

    public static String configuredArrowId() {
        return configuredArrow.getUniqueId().toString();
    }

    public static void assertConfiguredArrowAttached(int x, int y, int z) {
        if (!configuredArrow.isInBlock()) {
            throw new AssertionError("a block-hit Arrow must report isInBlock=true");
        }
        if (configuredArrow.getHitSound() != org.bukkit.Sound.ENTITY_ARROW_HIT) {
            throw new AssertionError("block impact must reset the configured hit sound");
        }
        java.util.List<org.bukkit.block.Block> attached = configuredArrow.getAttachedBlocks();
        org.bukkit.block.Block first = configuredArrow.getAttachedBlock();
        if (attached.size() != 1 || first == null
                || attached.get(0).getX() != x || attached.get(0).getY() != y
                || attached.get(0).getZ() != z || !first.equals(attached.get(0))) {
            throw new AssertionError("attached blocks must reflect the live collision: " + attached);
        }
    }

    private static void assertConcurrentShooterSnapshots(org.bukkit.entity.Arrow arrow,
            org.bukkit.entity.AbstractCow cow,
            org.bukkit.projectiles.ProjectileSource customSource) {
        java.util.concurrent.atomic.AtomicReference<Throwable> failure =
            new java.util.concurrent.atomic.AtomicReference<>();
        java.util.concurrent.CountDownLatch start = new java.util.concurrent.CountDownLatch(1);
        Runnable customWriter = () -> {
            try {
                start.await();
                for (int i = 0; i < 2_000; i++) arrow.setShooter(customSource, false);
            } catch (Throwable error) {
                failure.compareAndSet(null, error);
            }
        };
        Runnable entityWriter = () -> {
            try {
                start.await();
                for (int i = 0; i < 2_000; i++) {
                    arrow.setShooter((org.bukkit.projectiles.ProjectileSource) cow, false);
                }
            } catch (Throwable error) {
                failure.compareAndSet(null, error);
            }
        };
        Thread first = new Thread(customWriter, "foton-custom-shooter-writer");
        Thread second = new Thread(entityWriter, "foton-entity-shooter-writer");
        first.start();
        second.start();
        start.countDown();
        for (int i = 0; i < 4_000; i++) {
            org.bukkit.projectiles.ProjectileSource observed = arrow.getShooter();
            if (observed != customSource && !cow.equals(observed)) {
                failure.compareAndSet(null, new AssertionError(
                    "concurrent shooter snapshot exposed a hybrid state: " + observed));
                break;
            }
        }
        try {
            first.join();
            second.join();
        } catch (InterruptedException error) {
            Thread.currentThread().interrupt();
            throw new AssertionError("concurrent shooter check was interrupted", error);
        }
        if (failure.get() != null) {
            throw new AssertionError("concurrent shooter transaction failed", failure.get());
        }
        arrow.setShooter((org.bukkit.projectiles.ProjectileSource) cow, false);
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
