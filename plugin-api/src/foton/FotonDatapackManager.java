package foton;

import io.papermc.paper.datapack.Datapack;
import io.papermc.paper.datapack.DatapackManager;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.List;

/** Native-backed datapack inventory; snapshots are immutable per call. */
public final class FotonDatapackManager implements DatapackManager {
    public static final FotonDatapackManager INSTANCE = new FotonDatapackManager();
    private FotonDatapackManager() {}

    @Override public Collection<Datapack> getPacks() {
        return snapshot(false);
    }

    @Override public Collection<Datapack> getEnabledPacks() {
        return snapshot(true);
    }

    private static Collection<Datapack> snapshot(boolean enabledOnly) {
        String[] records = Native.datapacks(enabledOnly);
        if (records == null || records.length == 0) return Collections.emptyList();
        List<Datapack> result = new ArrayList<>(records.length);
        for (String record : records) {
            if (record == null) continue;
            String[] fields = record.split("\\t", -1);
            if (fields.length != 3) continue;
            result.add(new FotonDatapack(fields[0], fields[1], Boolean.parseBoolean(fields[2])));
        }
        return Collections.unmodifiableList(result);
    }

    private record FotonDatapack(String name, String compatibility, boolean enabled) implements Datapack {
        @Override public String getName() { return name; }
        @Override public Compatibility getCompatibility() {
            try { return Compatibility.valueOf(compatibility); }
            catch (IllegalArgumentException exception) { return Compatibility.TOO_NEW; }
        }
        @Override public boolean isEnabled() { return enabled; }
    }
}
