package org.bukkit.inventory.meta;

import org.bukkit.Color;
import org.bukkit.map.MapView;

public final class SimpleMapMeta extends SimpleItemMeta implements MapMeta {
    private boolean scaling; private MapView mapView; private String locationName; private Color color;
    @Override public boolean isScaling() { return scaling; }
    @Override public void setScaling(boolean value) { scaling = value; }
    @Override public boolean hasMapView() { return mapView != null; }
    @Override public MapView getMapView() { return mapView; }
    @Override public void setMapView(MapView view) { mapView = view; }
    @Override public boolean hasLocationName() { return locationName != null; }
    @Override public String getLocationName() { return locationName; }
    @Override public void setLocationName(String name) { locationName = name; }
    @Override public boolean hasColor() { return color != null; }
    @Override public Color getColor() { return color; }
    @Override public void setColor(Color value) { color = value; }
    @Override public SimpleMapMeta clone() { SimpleMapMeta copy = (SimpleMapMeta) super.clone(); copy.mapView = mapView; return copy; }
}
