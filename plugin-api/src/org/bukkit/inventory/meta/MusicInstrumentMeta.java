package org.bukkit.inventory.meta;

import org.bukkit.MusicInstrument;

/** The sound a goat horn plays: its {@code instrument} component. */
public interface MusicInstrumentMeta extends ItemMeta {
    void setInstrument(MusicInstrument instrument);

    MusicInstrument getInstrument();

    @Override
    MusicInstrumentMeta clone();
}
