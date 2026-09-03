package org.bukkit.advancement;

import java.util.Collection;
import org.bukkit.Keyed;

/** A vanilla advancement definition. */
interface LegacyAdvancementDisplay {
    AdvancementDisplay getDisplay();
}

public interface Advancement extends Keyed, LegacyAdvancementDisplay {
    Collection<String> getCriteria();
    default io.papermc.paper.advancement.AdvancementDisplay getDisplay() { return null; }
}
