package foton;

import java.util.UUID;
import org.bukkit.Art;
import org.bukkit.entity.Painting;

/** Live Bukkit view of a vanilla painting. */
public final class FotonPainting extends FotonEntity implements Painting {
    public FotonPainting(UUID id) { super(id); }
    @Override public Art getArt() {
        String value = Native.paintingArt(getUniqueId().toString());
        return value == null ? null : Art.getByName(value);
    }
    @Override public boolean setArt(Art art) { return setArt(art, false); }
    @Override public boolean setArt(Art art, boolean force) {
        return art != null && Native.setPaintingArt(getUniqueId().toString(), art.name(), force);
    }
}
