package org.bukkit.util;

/** Small numeric helpers retained for Bukkit source compatibility. */
public final class NumberConversions {
    private NumberConversions() {}
    public static double square(double value) { return value * value; }
    public static int floor(double value) { return (int) Math.floor(value); }
    public static int ceil(double value) { return (int) Math.ceil(value); }
    public static int round(double value) { return (int) Math.round(value); }
    public static int toInt(Object value) { return value instanceof Number n ? n.intValue() : Integer.parseInt(String.valueOf(value)); }
    public static float toFloat(Object value) { return value instanceof Number n ? n.floatValue() : Float.parseFloat(String.valueOf(value)); }
    public static double toDouble(Object value) { return value instanceof Number n ? n.doubleValue() : Double.parseDouble(String.valueOf(value)); }
    public static long toLong(Object value) { return value instanceof Number n ? n.longValue() : Long.parseLong(String.valueOf(value)); }
    public static boolean isFinite(double value) { return Double.isFinite(value); }
    public static boolean isFinite(float value) { return Float.isFinite(value); }
}
