package io.papermc.paper.registry.event;

import io.papermc.paper.registry.TypedKey;
import io.papermc.paper.registry.data.EnchantmentRegistryEntry;
import java.util.ArrayList;
import java.util.Queue;
import java.util.concurrent.ConcurrentLinkedQueue;
import org.bukkit.enchantments.Enchantment;

/** Internal hand-off from Paper's lifecycle callback to Steel's registry layer. */
public final class PluginEnchantmentQueue {
    private static final Queue<QueuedEnchantment> QUEUE = new ConcurrentLinkedQueue<>();

    private PluginEnchantmentQueue() {}

    /** Queues an immutable copy so later builder mutations cannot alter registration data. */
    public static void queue_plugin_enchantment(TypedKey<Enchantment> key, EnchantmentRegistryEntry.Builder builder) {
        QUEUE.add(new QueuedEnchantment(key, new Snapshot(builder)));
    }

    public static QueuedEnchantment poll() {
        return QUEUE.poll();
    }

    public static ArrayList<QueuedEnchantment> drain() {
        ArrayList<QueuedEnchantment> entries = new ArrayList<>();
        for (QueuedEnchantment entry; (entry = QUEUE.poll()) != null;) entries.add(entry);
        return entries;
    }

    public record QueuedEnchantment(TypedKey<Enchantment> key, EnchantmentRegistryEntry entry) {}

    private static final class Snapshot implements EnchantmentRegistryEntry {
        private final EnchantmentRegistryEntry delegate;
        Snapshot(EnchantmentRegistryEntry delegate) { this.delegate = delegate; }
        @Override public net.kyori.adventure.text.Component description() { return delegate.description(); }
        @Override public io.papermc.paper.registry.set.RegistryKeySet<org.bukkit.inventory.ItemType> supportedItems() { return delegate.supportedItems(); }
        @Override public io.papermc.paper.registry.set.RegistryKeySet<org.bukkit.inventory.ItemType> primaryItems() { return delegate.primaryItems(); }
        @Override public int weight() { return delegate.weight(); }
        @Override public int maxLevel() { return delegate.maxLevel(); }
        @Override public EnchantmentCost minimumCost() { return delegate.minimumCost(); }
        @Override public EnchantmentCost maximumCost() { return delegate.maximumCost(); }
        @Override public int anvilCost() { return delegate.anvilCost(); }
        @Override public java.util.List<org.bukkit.inventory.EquipmentSlotGroup> activeSlots() { return java.util.List.copyOf(delegate.activeSlots()); }
        @Override public io.papermc.paper.registry.set.RegistryKeySet<Enchantment> exclusiveWith() { return delegate.exclusiveWith(); }
    }
}
