package org.bukkit.persistence;

/** Primitive/complex type pair used by Bukkit persistent data containers. */
public final class PersistentDataType<P, C> {
    public static final PersistentDataType<Boolean, Boolean> BOOLEAN = new PersistentDataType<>();
    public static final PersistentDataType<Byte, Byte> BYTE = new PersistentDataType<>();
    public static final PersistentDataType<Short, Short> SHORT = new PersistentDataType<>();
    public static final PersistentDataType<Integer, Integer> INTEGER = new PersistentDataType<>();
    public static final PersistentDataType<Long, Long> LONG = new PersistentDataType<>();
    public static final PersistentDataType<Float, Float> FLOAT = new PersistentDataType<>();
    public static final PersistentDataType<Double, Double> DOUBLE = new PersistentDataType<>();
    public static final PersistentDataType<String, String> STRING = new PersistentDataType<>();
    public static final PersistentDataType<byte[], byte[]> BYTE_ARRAY = new PersistentDataType<>();
    private PersistentDataType() {}
}
