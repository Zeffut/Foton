package org.bukkit;

public interface WorldBorder {
    Location getCenter();
    void setCenter(double x, double z);
    double getSize();
    void setSize(double size);
    default void setSize(double size, long seconds) { setSize(size); }
    default void changeSize(double oldSize, double newSize, long seconds) { setSize(newSize, seconds); }
    default void changeSize(double newSize, long seconds) { changeSize(getSize(), newSize, seconds); }
    default boolean isInside(Location location) { return false; }
    default void reset() { }
    default int getWarningDistance() { return 5; }
    default void setWarningDistance(int distance) { }
    default int getWarningTime() { return 15; }
    default void setWarningTime(int seconds) { }
    default int getWarningTimeTicks() { return getWarningTime() * 20; }
    default void setWarningTimeTicks(int ticks) { setWarningTime(Math.max(0, ticks / 20)); }
    default double getDamageAmount() { return 0.2; }
    default void setDamageAmount(double damage) { }
    default double getDamageBuffer() { return 0.0; }
    default void setDamageBuffer(double distance) { }
}
