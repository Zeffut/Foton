package org.bukkit.block.data;

import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.util.EnumSet;
import java.util.LinkedHashSet;
import java.util.Locale;
import java.util.Set;
import org.bukkit.Axis;
import org.bukkit.block.BlockFace;

/** A block state that implements every BlockData interface its properties allow.
 *
 * <p>The state is the text Foton hands over. A proxy gives it the interfaces
 * {@link BlockDataTypes} picks, and each getter or setter reads or writes the
 * property its name stands for, so a new interface needs no class of its own.
 * Setters check the value against what the block registry allows for that
 * property, as Paper's {@code CraftBlockData} does.</p>
 */
public final class PropertyBlockData implements InvocationHandler {
    private static final BlockFace[] ROTATIONS = {
        BlockFace.SOUTH, BlockFace.SOUTH_SOUTH_WEST, BlockFace.SOUTH_WEST, BlockFace.WEST_SOUTH_WEST,
        BlockFace.WEST, BlockFace.WEST_NORTH_WEST, BlockFace.NORTH_WEST, BlockFace.NORTH_NORTH_WEST,
        BlockFace.NORTH, BlockFace.NORTH_NORTH_EAST, BlockFace.NORTH_EAST, BlockFace.EAST_NORTH_EAST,
        BlockFace.EAST, BlockFace.EAST_SOUTH_EAST, BlockFace.SOUTH_EAST, BlockFace.SOUTH_SOUTH_EAST,
    };
    private static final BlockFace[] SIDES = {
        BlockFace.NORTH, BlockFace.EAST, BlockFace.SOUTH, BlockFace.WEST, BlockFace.UP, BlockFace.DOWN,
    };

    private final SimpleBlockData state;

    private PropertyBlockData(SimpleBlockData state) {
        this.state = state;
    }

    /** The block data for a state written {@code minecraft:name[props]}. */
    public static BlockData of(String text) {
        SimpleBlockData state = new SimpleBlockData(text);
        Class<?>[] types = BlockDataTypes.of(state.getMaterial().getKey().getKey(), state.properties());
        if (types.length == 1) return state;
        return (BlockData) Proxy.newProxyInstance(
            BlockData.class.getClassLoader(), types, new PropertyBlockData(state));
    }

    /** The data for a state a plugin wrote: properties it left out take their defaults. */
    public static BlockData parse(String text) {
        String full = text == null ? null : foton.Native.blockNormalize(text);
        return of(full == null ? text : full);
    }

    @Override
    public Object invoke(Object proxy, Method method, Object[] args) throws Throwable {
        String name = method.getName();
        switch (name) {
            case "getMaterial" -> { return state.getMaterial(); }
            case "getAsString" -> { return state.getAsString(); }
            case "clone" -> { return of(state.getAsString()); }
            case "toString" -> { return state.getAsString(); }
            case "hashCode" -> { return state.hashCode(); }
            case "equals" -> { return proxy == args[0] || state.equals(args[0]); }
            case "getHalf" -> { return state.propertyValue("half").matches("top|upper") ? Bisected.Half.TOP : Bisected.Half.BOTTOM; }
            case "setHalf" -> { setHalf((Bisected.Half) args[0]); return null; }
            case "getRotation" -> { return ROTATIONS[Math.floorMod(integer("rotation"), 16)]; }
            case "setRotation" -> { setRotation((BlockFace) args[0]); return null; }
            case "getFaces" -> { return faces(true); }
            case "getAllowedFaces" -> { return faces(false); }
            case "hasFace" -> { return faces(true).contains((BlockFace) args[0]); }
            case "setFace" -> { setFace((BlockFace) args[0], (Boolean) args[1]); return null; }
            case "getAxes" -> { return axes(); }
            case "getAttachedFace" -> { return enumValue(method.getReturnType(), "face"); }
            case "setAttachedFace" -> { setEnum("face", args[0]); return null; }
            default -> { }
        }
        String property = propertyOf(name);
        if (property == null || !state.properties().containsKey(property)) {
            if (method.isDefault()) return InvocationHandler.invokeDefault(proxy, method, args);
            throw new UnsupportedOperationException(name + " has no property on " + state.getAsString());
        }
        if (name.startsWith("getMaximum")) return maximum(property);
        Class<?> type = method.getParameterCount() == 0 ? method.getReturnType() : method.getParameterTypes()[0];
        if (method.getParameterCount() == 0) {
            if (type == boolean.class) return Boolean.parseBoolean(state.propertyValue(property));
            if (type == int.class) return integer(property);
            if (type == BlockFace.class) return BlockFace.valueOf(state.propertyValue(property).toUpperCase(Locale.ROOT));
            return enumValue(type, property);
        }
        if (type == boolean.class) state.property(property, (boolean) args[0]);
        else if (type == int.class) setChecked(property, Integer.toString((int) args[0]));
        else setEnum(property, args[0]);
        return null;
    }

    /** The property a getter, setter or {@code is} method reads, or null if the name is none of these. */
    private static String propertyOf(String method) {
        String rest;
        if (method.startsWith("getMaximum")) rest = method.substring(10);
        else if (method.startsWith("get") || method.startsWith("set")) rest = method.substring(3);
        else if (method.startsWith("is")) rest = method.substring(2);
        else return null;
        if (rest.isEmpty()) return null;
        return rest.replaceAll("([a-z])([A-Z])", "$1_$2").toLowerCase(Locale.ROOT);
    }

    private int integer(String property) {
        try {
            return Integer.parseInt(state.propertyValue(property));
        } catch (NumberFormatException missing) {
            return 0;
        }
    }

    private Object enumValue(Class<?> type, String property) {
        String value = state.propertyValue(property).toUpperCase(Locale.ROOT);
        @SuppressWarnings({"unchecked", "rawtypes"})
        Object result = Enum.valueOf((Class) type, value);
        return result;
    }

    private void setEnum(String property, Object value) {
        if (value == null) throw new IllegalArgumentException(property + " cannot be null");
        String text = value instanceof Enum<?> e ? e.name() : value.toString();
        setChecked(property, text.toLowerCase(Locale.ROOT));
    }

    private void setChecked(String property, String value) {
        String[] allowed = allowed(property);
        if (allowed != null && !java.util.Arrays.asList(allowed).contains(value)) {
            throw new IllegalArgumentException(property + " cannot be " + value + ", expected one of "
                + String.join(", ", allowed));
        }
        state.property(property, value);
    }

    private String[] allowed(String property) {
        return foton.Native.blockPropertyValues(state.getMaterial().getKey().toString(), property);
    }

    private int maximum(String property) {
        int maximum = 0;
        String[] values = allowed(property);
        if (values == null) return integer(property);
        for (String value : values) {
            try { maximum = Math.max(maximum, Integer.parseInt(value)); } catch (NumberFormatException ignored) { }
        }
        return maximum;
    }

    /** Vanilla's {@code half} is top/bottom on stairs and upper/lower on doors and tall plants. */
    private void setHalf(Bisected.Half half) {
        if (half == null) throw new IllegalArgumentException("half cannot be null");
        boolean upperLower = state.propertyValue("half").matches("upper|lower");
        boolean top = half == Bisected.Half.TOP;
        state.property("half", upperLower ? (top ? "upper" : "lower") : (top ? "top" : "bottom"));
    }

    private void setRotation(BlockFace face) {
        for (int i = 0; i < ROTATIONS.length; i++) {
            if (ROTATIONS[i] == face) { state.property("rotation", Integer.toString(i)); return; }
        }
        throw new IllegalArgumentException("Not a rotation: " + face);
    }

    /** The sides the state has a flag for, or only the lit ones. */
    private Set<BlockFace> faces(boolean litOnly) {
        Set<BlockFace> result = new LinkedHashSet<>();
        for (BlockFace side : SIDES) {
            String value = state.propertyValue(side.name().toLowerCase(Locale.ROOT));
            if (value.isEmpty() || litOnly && !value.equals("true")) continue;
            result.add(side);
        }
        return result;
    }

    private void setFace(BlockFace face, boolean has) {
        if (face == null || !faces(false).contains(face)) {
            throw new IllegalArgumentException("Invalid face, must be one of " + faces(false));
        }
        state.property(face.name().toLowerCase(Locale.ROOT), has);
    }

    private Set<Axis> axes() {
        Set<Axis> result = EnumSet.noneOf(Axis.class);
        String[] values = allowed("axis");
        if (values != null) for (String value : values) result.add(Axis.valueOf(value.toUpperCase(Locale.ROOT)));
        return java.util.Collections.unmodifiableSet(result);
    }
}
