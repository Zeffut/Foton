package org.bukkit.inventory.meta;

import org.bukkit.Color;
import org.bukkit.map.MapView;

public interface MapMeta extends ItemMeta {
    boolean isScaling();
    void setScaling(boolean value);
    boolean hasMapView();
    MapView getMapView();
    void setMapView(MapView view);
    boolean hasLocationName();
    String getLocationName();
    void setLocationName(String name);
    boolean hasColor();
    Color getColor();
    void setColor(Color color);
    @Override MapMeta clone();
    default java.util.Map<String,Object> serialize() { return java.util.Collections.emptyMap(); }
}
