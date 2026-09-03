package foton;

import java.util.Arrays;
import java.util.Collection;
import org.bukkit.NamespacedKey;

/** Bukkit view of a generated Steel advancement definition. */
public final class FotonAdvancement implements org.bukkit.advancement.Advancement {
    private final NamespacedKey key;
    private final String[] criteria;
    private final io.papermc.paper.advancement.AdvancementDisplay display;

    public FotonAdvancement(NamespacedKey key, String[] criteria) {
        this.key = key;
        this.criteria = criteria == null ? new String[0] : criteria.clone();
        String[] info = Native.advancementDisplay(key.toString());
        this.display = info == null || info.length < 4 ? null : new io.papermc.paper.advancement.AdvancementDisplay(info[0], info[1], Boolean.parseBoolean(info[2]), Boolean.parseBoolean(info[3]));
    }

    @Override public NamespacedKey getKey() { return key; }
    @Override public io.papermc.paper.advancement.AdvancementDisplay getDisplay() { return display; }
    @Override public Collection<String> getCriteria() {
        return java.util.Collections.unmodifiableList(Arrays.asList(criteria.clone()));
    }
}
