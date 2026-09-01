package org.bukkit;

public interface WorldBorder {
    Location getCenter();
    void setCenter(double x, double z);
    double getSize();
    void setSize(double size);
}
