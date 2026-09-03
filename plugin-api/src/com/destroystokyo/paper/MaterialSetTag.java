package com.destroystokyo.paper;

import java.util.Collection;
import java.util.Collections;
import java.util.HashSet;
import java.util.Set;
import org.bukkit.Material;

public final class MaterialSetTag {
    private final Set<Material> values;
    public MaterialSetTag(Collection<Material> values) {
        this.values = values == null ? Collections.emptySet() : Collections.unmodifiableSet(new HashSet<>(values));
    }
    public boolean isTagged(Material material) { return material != null && values.contains(material); }
}
