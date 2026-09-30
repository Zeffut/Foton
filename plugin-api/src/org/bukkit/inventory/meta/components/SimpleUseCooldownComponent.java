package org.bukkit.inventory.meta.components;

import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Objects;
import org.bukkit.NamespacedKey;

/** A use cooldown as a value: a meta hands out copies, as Paper's does. */
public final class SimpleUseCooldownComponent implements UseCooldownComponent {
    private float seconds;
    private NamespacedKey group;

    public SimpleUseCooldownComponent(float seconds, NamespacedKey group) {
        this.seconds = seconds;
        this.group = group;
    }

    @Override public float getCooldownSeconds() { return seconds; }

    @Override
    public void setCooldownSeconds(float cooldown) {
        if (!(cooldown > 0)) throw new IllegalArgumentException("cooldown must be positive, got " + cooldown);
        seconds = cooldown;
    }

    @Override public NamespacedKey getCooldownGroup() { return group; }
    @Override public void setCooldownGroup(NamespacedKey value) { group = value; }

    public SimpleUseCooldownComponent copy() {
        return new SimpleUseCooldownComponent(seconds, group);
    }

    @Override
    public Map<String, Object> serialize() {
        Map<String, Object> out = new LinkedHashMap<>();
        out.put("seconds", seconds);
        if (group != null) out.put("cooldown-group", group.toString());
        return out;
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof SimpleUseCooldownComponent that
            && Float.compare(seconds, that.seconds) == 0 && Objects.equals(group, that.group);
    }

    @Override
    public int hashCode() {
        return Objects.hash(seconds, group);
    }
}
