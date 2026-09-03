package org.bukkit.profile;

import java.net.URL;

/** Mutable texture URLs associated with a player profile. */
public interface PlayerTextures {
    URL getSkin();
    void setSkin(URL skin);
    default void setSkin(URL skin, boolean signed) { setSkin(skin); }
    URL getCape();
    void setCape(URL cape);
    String getSkinModel();
    void setSkinModel(String model);
    void clear();
}
