/** Existing native entity state exposed through the Bukkit surface. */
final class EntityCheck {
    private EntityCheck() {}

    static void check() {
        suppliedRegistryTypeSelectsTheWrapperWithoutAnotherLookup();
        generatedClassLookupPreservesUnderscoredRegistryKeys();
        generatedClassLookupUsesCanonicalPaperClasses();
        generatedClassLookupPreservesPaperDefaultsAndAmbiguity();
        generatedClassLookupPreservesPaperAbstractAndSpecializedDefaults();
        generatedWrapperSupportMatchesRuntimeWrappers();
        entityContractsPreservePaperInheritance();
        entityAbiMatchesPaper26();
        transformReasonsMatchPaper26();
        paperDeprecationMetadataIsExact();
        dimensionsComeFromTheBoundingBox();
        destinationLocationIsFilledInPlace();
        missingEntityStillReturnsDestination();
    }

    private static void entityAbiMatchesPaper26() {
        assertEntityMethod("getPassengers", java.util.List.class);
        assertEntityMethod("getNearbyEntities", java.util.List.class,
            double.class, double.class, double.class);
        assertEntityMethod("getFallDistance", float.class);
        assertEntityMethod("setFallDistance", void.class, float.class);
    }

    private static void assertEntityMethod(String name, Class<?> returnType,
            Class<?>... parameterTypes) {
        try {
            java.lang.reflect.Method method = org.bukkit.entity.Entity.class
                .getDeclaredMethod(name, parameterTypes);
            Checks.same(method.getReturnType(), returnType,
                "Entity." + name + " return type");
            Checks.expect(java.lang.reflect.Modifier.isAbstract(method.getModifiers()),
                "Entity." + name + " must be an abstract Paper contract");
        } catch (ReflectiveOperationException error) {
            throw new AssertionError("Entity." + name + " Paper ABI is missing", error);
        }
    }

    private static void transformReasonsMatchPaper26() {
        Checks.same(java.util.Arrays.stream(
                org.bukkit.event.entity.EntityTransformEvent.TransformReason.values())
                .map(Enum::name).toList(),
            java.util.List.of("CURED", "FROZEN", "INFECTION", "DROWNED", "SHEARED",
                "LIGHTNING", "SPLIT", "PIGLIN_ZOMBIFIED", "METAMORPHOSIS", "UNKNOWN"),
            "EntityTransformEvent Paper reasons");
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

    private static void generatedWrapperSupportMatchesRuntimeWrappers() {
        Checks.expect(foton.FotonEntityFactory.supportsSpawn(
                org.bukkit.entity.AbstractCow.class, org.bukkit.entity.EntityType.COW),
            "AbstractCow must be supported by the generated Cow wrapper metadata");
        Checks.expect(foton.FotonEntityFactory.supportsSpawn(
                org.bukkit.entity.TippedArrow.class, org.bukkit.entity.EntityType.ARROW),
            "TippedArrow must use its specialized generated wrapper metadata");
        Checks.expect(!foton.FotonEntityFactory.supportsSpawn(
                org.bukkit.entity.Egg.class, org.bukkit.entity.EntityType.EGG),
            "Egg must not be spawnable until its runtime wrapper implements Egg");
    }

    private static void entityContractsPreservePaperInheritance() {
        Checks.expect(java.util.Arrays.equals(org.bukkit.entity.Ageable.class.getInterfaces(),
                new Class<?>[] { org.bukkit.entity.Creature.class }),
            "Ageable must directly extend Creature");
        Checks.expect(java.util.Arrays.equals(org.bukkit.entity.Breedable.class.getInterfaces(),
                new Class<?>[] { org.bukkit.entity.Ageable.class }),
            "Breedable must directly extend Ageable");
        Checks.expect(java.util.Arrays.equals(org.bukkit.entity.Animals.class.getInterfaces(),
                new Class<?>[] { org.bukkit.entity.Breedable.class }),
            "Animals must directly extend Breedable");
        Checks.expect(java.util.Arrays.equals(org.bukkit.entity.AbstractCow.class.getInterfaces(),
                new Class<?>[] { org.bukkit.entity.Animals.class }),
            "AbstractCow must directly extend Paper's Animals contract");
        Checks.expect(java.util.Arrays.equals(org.bukkit.entity.Cow.class.getInterfaces(),
                new Class<?>[] { org.bukkit.entity.AbstractCow.class }),
            "Cow must directly extend AbstractCow");
        Checks.expect(org.bukkit.entity.Creature.class.isAssignableFrom(
                org.bukkit.entity.Cow.class),
            "Cow must transitively inherit Creature");
        Checks.expect(org.bukkit.entity.Mob.class.isAssignableFrom(
                org.bukkit.entity.Cow.class),
            "Cow must transitively inherit Mob");
        Checks.expect(java.util.Arrays.stream(org.bukkit.entity.Ageable.class.getDeclaredMethods())
                .noneMatch(method -> method.getName().equals("setBaby")
                    && method.getParameterCount() == 1),
            "Ageable must not add a setBaby(boolean) declaration absent from Paper");
        Checks.expect(java.util.Arrays.asList(org.bukkit.entity.MushroomCow.class.getInterfaces())
                .equals(java.util.List.of(org.bukkit.entity.AbstractCow.class,
                    io.papermc.paper.entity.Shearable.class)),
            "MushroomCow must directly extend AbstractCow and Paper Shearable");
        Checks.expect(org.bukkit.entity.Creature.class.isAssignableFrom(
                org.bukkit.entity.AbstractCubeMob.class),
            "AbstractCubeMob must inherit Creature");
        Checks.expect(org.bukkit.entity.AbstractCubeMob.class.isAssignableFrom(
                org.bukkit.entity.Slime.class),
            "Slime must inherit AbstractCubeMob");
        Checks.expect(org.bukkit.entity.Enemy.class.isAssignableFrom(
                org.bukkit.entity.Slime.class),
            "Slime must implement Enemy");
        Checks.expect(java.util.Arrays.stream(org.bukkit.entity.Slime.class.getDeclaredMethods())
                .noneMatch(method -> method.getName().equals("getSize")),
            "Slime must inherit getSize without redeclaring it");
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

    private static void paperDeprecationMetadataIsExact() {
        for (String name : java.util.List.of("canBreed", "setBreed")) {
            try {
                Class<?>[] parameters = name.equals("setBreed")
                    ? new Class<?>[] { boolean.class } : new Class<?>[0];
                Deprecated deprecated = org.bukkit.entity.Ageable.class
                    .getDeclaredMethod(name, parameters)
                    .getAnnotation(Deprecated.class);
                Checks.expect(deprecated != null && deprecated.since().equals("1.16.2")
                        && !deprecated.forRemoval(),
                    "Ageable." + name + " must carry Paper's 1.16.2 deprecation metadata");
            } catch (ReflectiveOperationException error) {
                throw new AssertionError("Ageable breeding ABI is missing", error);
            }
        }
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

    @SuppressWarnings("unused")
    private static void compileAnimalsSurface(org.bukkit.entity.Animals animal,
            java.util.UUID cause, org.bukkit.inventory.ItemStack stack,
            org.bukkit.Material material) {
        java.util.UUID currentCause = animal.getBreedCause();
        animal.setBreedCause(cause);
        boolean loveMode = animal.isLoveMode();
        int loveTicks = animal.getLoveModeTicks();
        animal.setLoveModeTicks(loveTicks);
        boolean stackFood = animal.isBreedItem(stack);
        boolean materialFood = animal.isBreedItem(material);
    }

    @SuppressWarnings("unused")
    private static void compileCowSurface(org.bukkit.entity.Cow cow) {
        org.bukkit.entity.Cow.Variant variant = cow.getVariant();
        cow.setVariant(org.bukkit.entity.Cow.Variant.COLD);
        org.bukkit.entity.Cow.SoundVariant sounds = cow.getSoundVariant();
        cow.setSoundVariant(org.bukkit.entity.Cow.SoundVariant.MOODY);
        org.bukkit.NamespacedKey variantKey = variant.getKey();
        org.bukkit.NamespacedKey soundKey = sounds.getKey();
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
