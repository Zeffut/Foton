package foton;

import java.net.URL;
import java.util.UUID;
import org.bukkit.profile.PlayerTextures;

public final class FotonPlayerProfile implements com.destroystokyo.paper.profile.PlayerProfile {
    private final UUID id; private final String name; private PlayerTextures textures = new Textures();
    public FotonPlayerProfile(UUID id, String name) { this.id = id; this.name = name; }
    @Override public UUID getUniqueId() { return id; }
    @Override public String getName() { return name; }
    @Override public PlayerTextures getTextures() { return textures; }
    @Override public void setTextures(PlayerTextures value) { textures = value == null ? new Textures() : value; }
    @Override public boolean isComplete() { return id != null || name != null; }
    private static final class Textures implements PlayerTextures {
        private URL skin, cape; private String model;
        @Override public URL getSkin() { return skin; }
        @Override public void setSkin(URL value) { skin = value; }
        @Override public URL getCape() { return cape; }
        @Override public void setCape(URL value) { cape = value; }
        @Override public String getSkinModel() { return model; }
        @Override public void setSkinModel(String value) { model = value; }
        @Override public void clear() { skin = null; cape = null; model = null; }
    }
}
