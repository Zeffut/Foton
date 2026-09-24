package org.bukkit.persistence;

/** How a complex value is stored as one of the NBT primitives a container holds.
 *
 * <p>An interface, as in Paper: plugins implement it for their own types, and
 * the container stores only the primitive {@link #toPrimitive} returns.</p>
 */
public interface PersistentDataType<P, C> {
    PersistentDataType<Byte, Byte> BYTE = new PrimitivePersistentDataType<>(Byte.class);
    PersistentDataType<Short, Short> SHORT = new PrimitivePersistentDataType<>(Short.class);
    PersistentDataType<Integer, Integer> INTEGER = new PrimitivePersistentDataType<>(Integer.class);
    PersistentDataType<Long, Long> LONG = new PrimitivePersistentDataType<>(Long.class);
    PersistentDataType<Float, Float> FLOAT = new PrimitivePersistentDataType<>(Float.class);
    PersistentDataType<Double, Double> DOUBLE = new PrimitivePersistentDataType<>(Double.class);
    /** Stored as a byte, as in Paper, so a value written as BYTE 1 reads as true. */
    PersistentDataType<Byte, Boolean> BOOLEAN = new BooleanPersistentDataType();
    PersistentDataType<String, String> STRING = new PrimitivePersistentDataType<>(String.class);
    PersistentDataType<byte[], byte[]> BYTE_ARRAY = new PrimitivePersistentDataType<>(byte[].class);
    PersistentDataType<int[], int[]> INTEGER_ARRAY = new PrimitivePersistentDataType<>(int[].class);
    PersistentDataType<long[], long[]> LONG_ARRAY = new PrimitivePersistentDataType<>(long[].class);
    PersistentDataType<PersistentDataContainer, PersistentDataContainer> TAG_CONTAINER =
        new PrimitivePersistentDataType<>(PersistentDataContainer.class);

    Class<P> getPrimitiveType();

    Class<C> getComplexType();

    P toPrimitive(C complex, PersistentDataAdapterContext context);

    C fromPrimitive(P primitive, PersistentDataAdapterContext context);

    class PrimitivePersistentDataType<P> implements PersistentDataType<P, P> {
        private final Class<P> primitiveType;

        PrimitivePersistentDataType(Class<P> primitiveType) {
            this.primitiveType = primitiveType;
        }

        @Override public Class<P> getPrimitiveType() { return primitiveType; }
        @Override public Class<P> getComplexType() { return primitiveType; }
        @Override public P toPrimitive(P complex, PersistentDataAdapterContext context) { return complex; }
        @Override public P fromPrimitive(P primitive, PersistentDataAdapterContext context) { return primitive; }
    }

    class BooleanPersistentDataType implements PersistentDataType<Byte, Boolean> {
        @Override public Class<Byte> getPrimitiveType() { return Byte.class; }
        @Override public Class<Boolean> getComplexType() { return Boolean.class; }
        @Override public Byte toPrimitive(Boolean complex, PersistentDataAdapterContext context) { return (byte) (complex ? 1 : 0); }
        @Override public Boolean fromPrimitive(Byte primitive, PersistentDataAdapterContext context) { return primitive != 0; }
    }
}
