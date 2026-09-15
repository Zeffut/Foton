/** Existing native entity state exposed through the Bukkit surface. */
final class EntityCheck {
    private EntityCheck() {}

    static void check() {
        dimensionsComeFromTheBoundingBox();
        destinationLocationIsFilledInPlace();
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
}
