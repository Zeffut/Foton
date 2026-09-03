package io.papermc.paper.math;

/** Paper's immutable three-dimensional position factories and common contract. */
public interface Position {
    static FinePosition fine(double x, double y, double z) {
        return new FinePositionImpl(x, y, z);
    }

    static BlockPosition block(int x, int y, int z) {
        return new BlockPosition(x, y, z);
    }

    double x();
    double y();
    double z();

    default int blockX() { return (int) Math.floor(x()); }
    default int blockY() { return (int) Math.floor(y()); }
    default int blockZ() { return (int) Math.floor(z()); }
    default boolean isBlock() { return this instanceof BlockPosition; }
    default boolean isFine() { return this instanceof FinePosition; }
}

final class FinePositionImpl implements FinePosition {
    private final double x;
    private final double y;
    private final double z;

    FinePositionImpl(double x, double y, double z) {
        this.x = x;
        this.y = y;
        this.z = z;
    }

    @Override public double x() { return x; }
    @Override public double y() { return y; }
    @Override public double z() { return z; }
}
