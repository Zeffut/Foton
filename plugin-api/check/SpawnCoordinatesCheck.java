/** Runs without native bindings: invalid input must never reach JNI. */
public final class SpawnCoordinatesCheck {
    public static void main(String[] args) {
        var world = foton.FotonWorld.of("minecraft:overworld");
        int rejected = 0;
        for (int axis = 0; axis < 3; axis++) {
            for (double value : new double[] { Double.NaN, Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY }) {
                double[] coordinates = { 8.5, 64.0, 8.5 };
                coordinates[axis] = value;
                var location = new org.bukkit.Location(world, coordinates[0], coordinates[1], coordinates[2]);
                for (boolean consumer : new boolean[] { false, true }) {
                    try {
                        if (consumer) {
                            world.spawn(location, org.bukkit.entity.Arrow.class, entity -> {
                                throw new AssertionError("invalid spawn reached the consumer");
                            });
                        } else {
                            world.spawn(location, org.bukkit.entity.Arrow.class);
                        }
                        throw new AssertionError("non-finite coordinate accepted: " + axis + " / " + value);
                    } catch (IllegalArgumentException expected) {
                        rejected++;
                    }
                }
            }
        }
        if (rejected != 18) throw new AssertionError("not every invalid spawn was rejected");
        System.out.println("non-finite spawn validation: 18 Java calls rejected before JNI; process survived");
    }
}
