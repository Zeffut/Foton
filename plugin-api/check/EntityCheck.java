/** Existing native entity state exposed through the Bukkit surface. */
final class EntityCheck {
    private EntityCheck() {}

    static void check() {
        suppliedRegistryTypeSelectsTheWrapperWithoutAnotherLookup();
        generatedClassLookupPreservesUnderscoredRegistryKeys();
        generatedClassLookupUsesCanonicalPaperClasses();
        generatedClassLookupPreservesPaperDefaultsAndAmbiguity();
        generatedClassLookupPreservesPaperAbstractAndSpecializedDefaults();
        entityContractsPreservePaperInheritance();
        dimensionsComeFromTheBoundingBox();
        destinationLocationIsFilledInPlace();
        missingEntityStillReturnsDestination();
    }

    private static void generatedClassLookupPreservesUnderscoredRegistryKeys() {
        Checks.same(
            foton.FotonEntityFactory.typeFor(org.bukkit.entity.BlockDisplay.class),
            org.bukkit.entity.EntityType.BLOCK_DISPLAY,
            "BlockDisplay class-to-entity type");
    }

    private static void generatedClassLookupUsesCanonicalPaperClasses() {
        Checks.same(foton.FotonEntityFactory.typeFor(org.bukkit.entity.TNTPrimed.class),
            org.bukkit.entity.EntityType.TNT, "TNTPrimed class-to-entity type");
        Checks.same(foton.FotonEntityFactory.typeFor(org.bukkit.entity.EnderCrystal.class),
            org.bukkit.entity.EntityType.END_CRYSTAL, "EnderCrystal class-to-entity type");
        Checks.same(foton.FotonEntityFactory.typeFor(org.bukkit.entity.Firework.class),
            org.bukkit.entity.EntityType.FIREWORK_ROCKET, "Firework class-to-entity type");
        Checks.same(foton.FotonEntityFactory.typeFor(org.bukkit.entity.MushroomCow.class),
            org.bukkit.entity.EntityType.MOOSHROOM, "MushroomCow class-to-entity type");
        Checks.same(foton.FotonEntityFactory.typeFor(org.bukkit.entity.ThrownExpBottle.class),
            org.bukkit.entity.EntityType.EXPERIENCE_BOTTLE,
            "ThrownExpBottle class-to-entity type");
        Checks.same(foton.FotonEntityFactory.typeFor(org.bukkit.entity.FishHook.class),
            org.bukkit.entity.EntityType.FISHING_BOBBER, "FishHook class-to-entity type");
    }

    private static void generatedClassLookupPreservesPaperDefaultsAndAmbiguity() {
        Checks.same(foton.FotonEntityFactory.typeFor(org.bukkit.entity.AbstractArrow.class),
            org.bukkit.entity.EntityType.ARROW, "AbstractArrow defaults to Arrow");
        Checks.same(foton.FotonEntityFactory.typeFor(org.bukkit.entity.AbstractHorse.class),
            org.bukkit.entity.EntityType.HORSE, "AbstractHorse defaults to Horse");
        Checks.same(foton.FotonEntityFactory.typeFor(org.bukkit.entity.Fireball.class),
            org.bukkit.entity.EntityType.FIREBALL, "Fireball defaults to the large fireball type");
        Checks.same(foton.FotonEntityFactory.typeFor(org.bukkit.entity.Minecart.class),
            org.bukkit.entity.EntityType.MINECART, "Minecart defaults to the rideable type");
        Checks.same(foton.FotonEntityFactory.typeFor(org.bukkit.entity.ThrownPotion.class),
            org.bukkit.entity.EntityType.SPLASH_POTION,
            "ThrownPotion defaults to the splash potion type");
        Checks.same(foton.FotonEntityFactory.typeFor(org.bukkit.entity.Boat.class), null,
            "Boat is ambiguous across registry variants");
        Checks.same(foton.FotonEntityFactory.typeFor(org.bukkit.entity.Fish.class), null,
            "Fish is ambiguous across registry variants");
    }

    private static void generatedClassLookupPreservesPaperAbstractAndSpecializedDefaults() {
        Checks.same(foton.FotonEntityFactory.typeFor(org.bukkit.entity.AbstractCow.class),
            org.bukkit.entity.EntityType.COW, "AbstractCow defaults to Cow");
        Checks.same(foton.FotonEntityFactory.typeFor(org.bukkit.entity.AbstractCubeMob.class),
            org.bukkit.entity.EntityType.SLIME, "AbstractCubeMob defaults to Slime");
        Checks.same(foton.FotonEntityFactory.typeFor(org.bukkit.entity.SizedFireball.class),
            org.bukkit.entity.EntityType.FIREBALL, "SizedFireball defaults to Fireball");
        Checks.same(foton.FotonEntityFactory.typeFor(org.bukkit.entity.TippedArrow.class),
            org.bukkit.entity.EntityType.ARROW, "TippedArrow defaults to Arrow");
    }

    private static void entityContractsPreservePaperInheritance() {
        Checks.expect(org.bukkit.entity.Animal.class.isAssignableFrom(
                org.bukkit.entity.AbstractCow.class),
            "AbstractCow must preserve Foton's animal hierarchy");
        Checks.expect(org.bukkit.entity.AbstractCow.class.isAssignableFrom(
                org.bukkit.entity.Cow.class),
            "Cow must inherit AbstractCow");
        Checks.expect(org.bukkit.entity.Creature.class.isAssignableFrom(
                org.bukkit.entity.AbstractCubeMob.class),
            "AbstractCubeMob must inherit Creature");
        Checks.expect(org.bukkit.entity.AbstractCubeMob.class.isAssignableFrom(
                org.bukkit.entity.Slime.class),
            "Slime must inherit AbstractCubeMob");
        Checks.expect(org.bukkit.entity.Fireball.class.isAssignableFrom(
                org.bukkit.entity.SizedFireball.class),
            "SizedFireball must inherit Fireball");
        Checks.expect(org.bukkit.entity.SizedFireball.class.isAssignableFrom(
                foton.FotonSizedFireball.class),
            "the sized fireball wrapper must expose SizedFireball");
        Checks.expect(org.bukkit.entity.Arrow.class.isAssignableFrom(
                org.bukkit.entity.TippedArrow.class),
            "TippedArrow must inherit Arrow");
        Checks.expect(org.bukkit.entity.TippedArrow.class.isAssignableFrom(
                foton.FotonTippedArrow.class),
            "the tipped arrow wrapper must expose TippedArrow");
        Checks.expect(!org.bukkit.entity.TippedArrow.class.isAssignableFrom(
                foton.FotonArrow.class),
            "ordinary arrows must not claim tipped-arrow identity");
    }

    private static void suppliedRegistryTypeSelectsTheWrapperWithoutAnotherLookup() {
        java.util.UUID id =
            java.util.UUID.fromString("00000000-0000-0000-0000-000000000001");
        org.bukkit.entity.Entity wrapped;
        try {
            java.lang.reflect.Method method = foton.FotonWorld.class.getDeclaredMethod(
                "wrapEntity", java.util.UUID.class, String.class);
            method.setAccessible(true);
            wrapped = (org.bukkit.entity.Entity) method.invoke(null, id, "block_display");
        } catch (java.lang.reflect.InvocationTargetException error) {
            if (error.getCause() instanceof Error cause) throw cause;
            if (error.getCause() instanceof RuntimeException cause) throw cause;
            throw new AssertionError("entity wrapper failed", error.getCause());
        } catch (ReflectiveOperationException error) {
            throw new AssertionError("entity wrapper contract is missing", error);
        }

        Checks.same(wrapped.getUniqueId(), id, "entity wrapper UUID");
        Checks.expect(wrapped instanceof foton.FotonBlockDisplay,
            "block_display should use its typed wrapper");
    }

    private static void dimensionsComeFromTheBoundingBox() {
        DerivedEntity entity = new DerivedEntity();

        Checks.same(entity.getWidth(), 3.0, "entity width comes from its current bounds");
        Checks.same(entity.getHeight(), 5.0, "entity height comes from its current bounds");
    }

    private static void destinationLocationIsFilledInPlace() {
        DerivedEntity entity = new DerivedEntity();
        org.bukkit.Location destination = new org.bukkit.Location(null, 0.0, 0.0, 0.0);

        Checks.expect(entity.getLocation(destination) == destination,
            "getLocation(destination) should return the supplied object");
        Checks.same(destination.getX(), 1.25, "destination x");
        Checks.same(destination.getY(), 64.5, "destination y");
        Checks.same(destination.getZ(), -3.75, "destination z");
        Checks.same(destination.getYaw(), 120.0f, "destination yaw");
        Checks.same(destination.getPitch(), -35.0f, "destination pitch");
        Checks.same(entity.getLocation(null), null,
            "a null destination should remain null");
    }

    private static void missingEntityStillReturnsDestination() {
        MissingEntity entity = new MissingEntity();
        org.bukkit.Location destination = new org.bukkit.Location(null, 3.0, 4.0, 5.0);

        Checks.expect(entity.getLocation(destination) == destination,
            "a missing entity should still return the supplied destination");
    }

    @SuppressWarnings("unused")
    private static void compileSurface(org.bukkit.entity.Entity entity) {
        java.util.Set<String> tags = entity.getScoreboardTags();
        boolean tagAdded = entity.addScoreboardTag("example");
        boolean tagRemoved = entity.removeScoreboardTag("example");
        double width = entity.getWidth();
        double height = entity.getHeight();
        org.bukkit.Location location = entity.getLocation(new org.bukkit.Location(null, 0, 0, 0));
        boolean gravity = entity.hasGravity();
        entity.setGravity(gravity);
        boolean silent = entity.isSilent();
        entity.setSilent(silent);
        entity.setRotation(90.0f, 30.0f);
        boolean raining = entity.isInRain();
    }

    private static final class DerivedEntity extends foton.FotonEntity {
        DerivedEntity() {
            super(java.util.UUID.fromString("00000000-0000-0000-0000-000000000002"));
        }

        @Override public org.bukkit.util.BoundingBox getBoundingBox() {
            return new org.bukkit.util.BoundingBox(1.0, 2.0, 3.0, 4.0, 7.0, 9.0);
        }

        @Override public org.bukkit.Location getLocation() {
            return new org.bukkit.Location(null, 1.25, 64.5, -3.75, 120.0f, -35.0f);
        }
    }

    private static final class MissingEntity extends foton.FotonEntity {
        MissingEntity() {
            super(java.util.UUID.fromString("00000000-0000-0000-0000-000000000003"));
        }

        @Override public org.bukkit.Location getLocation() {
            return null;
        }
    }
}
