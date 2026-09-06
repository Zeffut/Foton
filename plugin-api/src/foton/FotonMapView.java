package foton;

/** A map view with an id and nothing behind it.
 *
 * <p>Foton stores no map data. `Bukkit.createMap` still has to return
 * something, because a plugin calling it and getting null crashes on its next
 * line -- so this carries the identifier and admits, by having no other state,
 * that there is no map.
 */
public final class FotonMapView implements org.bukkit.map.MapView {
    private final int id;

    public FotonMapView(int id) {
        this.id = id;
    }

    @Override public int getId() { return id; }
}
