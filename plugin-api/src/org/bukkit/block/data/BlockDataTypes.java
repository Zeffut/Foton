package org.bukkit.block.data;

import java.util.LinkedHashSet;
import java.util.Map;
import java.util.Set;
import org.bukkit.block.data.type.*;

/** Which BlockData interfaces a block state implements.
 *
 * Paper has a class per block, so {@code CraftCampfire} is Lightable,
 * Directional and Waterlogged at once. Foton reads the same thing off the
 * state itself: a property the registry gave the block selects the interface
 * that exposes it, and the few interfaces that name a block rather than a
 * property ({@code Furnace}, {@code Door}, the signs) key on the block.
 */
final class BlockDataTypes {
    private static final Set<String> FURNACES = Set.of("furnace", "blast_furnace", "smoker");

    private BlockDataTypes() { }

    static Class<?>[] of(String block, Map<String, String> p) {
        Set<Class<?>> types = new LinkedHashSet<>();
        types.add(BlockData.class);
        if (isInteger(p.get("age"))) types.add(Ageable.class);
        if (p.containsKey("attached")) types.add(Attachable.class);
        if (p.containsKey("axis")) types.add(Orientable.class);
        if (p.containsKey("face")) types.add(FaceAttachable.class);
        if (p.containsKey("facing")) types.add(Directional.class);
        if (p.containsKey("half")) types.add(Bisected.class);
        if (isInteger(p.get("level"))) types.add(Levelled.class);
        if (p.containsKey("lit")) types.add(Lightable.class);
        if (p.containsKey("open")) types.add(Openable.class);
        if (isInteger(p.get("power"))) types.add(AnaloguePowerable.class);
        if (p.containsKey("powered")) types.add(Powerable.class);
        if (p.containsKey("rotation")) types.add(Rotatable.class);
        if (p.containsKey("snowy")) types.add(Snowable.class);
        if (p.containsKey("waterlogged")) types.add(Waterlogged.class);
        // Walls and redstone wire name their sides with an enum, not a flag.
        if (isBoolean(p.get("north"))) types.add(MultipleFacing.class);

        if (p.containsKey("attachment") && p.containsKey("facing")) types.add(Bell.class);
        if (p.containsKey("bites")) types.add(Cake.class);
        if (p.containsKey("hinge")) types.add(Door.class);
        if (p.containsKey("extended")) types.add(Piston.class);
        if (p.containsKey("hanging")) types.add(Lantern.class);
        if (p.containsKey("thickness")) types.add(PointedDripstone.class);
        if (p.containsKey("signal_fire")) types.add(Campfire.class);
        if (oneOf(p.get("type"), "single", "left", "right")) types.add(Chest.class);
        if (oneOf(p.get("part"), "head", "foot")) types.add(Bed.class);
        if (oneOf(p.get("shape"), "straight", "inner_left", "inner_right", "outer_left", "outer_right")) types.add(Stairs.class);
        if (FURNACES.contains(block)) types.add(Furnace.class);
        switch (block) {
            case "dispenser", "dropper" -> types.add(Dispenser.class);
            case "piston_head" -> types.add(PistonHead.class);
            case "moving_piston" -> types.add(TechnicalPiston.class);
            case "tripwire" -> types.add(Tripwire.class);
            default -> { }
        }
        if (block.endsWith("_wall_hanging_sign")) types.add(WallHangingSign.class);
        else if (block.endsWith("_hanging_sign")) types.add(HangingSign.class);
        else if (block.endsWith("_wall_sign")) types.add(WallSign.class);
        else if (block.endsWith("_sign")) types.add(Sign.class);
        return types.toArray(new Class<?>[0]);
    }

    private static boolean oneOf(String value, String... options) {
        return value != null && java.util.Arrays.asList(options).contains(value);
    }

    private static boolean isBoolean(String value) {
        return "true".equals(value) || "false".equals(value);
    }

    private static boolean isInteger(String value) {
        if (value == null || value.isEmpty()) return false;
        for (int i = 0; i < value.length(); i++) if (!Character.isDigit(value.charAt(i))) return false;
        return true;
    }
}
