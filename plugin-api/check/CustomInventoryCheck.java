package foton;

/** Custom menus must keep their Bukkit identity for the complete event lifetime. */
public final class CustomInventoryCheck {
    private CustomInventoryCheck() {}

    public static void check() {
        String id = "00000000-0000-0000-0000-000000000019";
        org.bukkit.inventory.Inventory[] held = {null};
        org.bukkit.inventory.InventoryHolder holder = () -> held[0];
        FotonCustomInventory inventory = new FotonCustomInventory(holder, 9, "custom");
        held[0] = inventory;
        inventory.attachViewer(id);

        FotonPlayer player = new FotonPlayer(java.util.UUID.fromString(id));
        expect(player.getOpenInventory().getTopInventory() == inventory,
            "the live view replaced the open custom inventory");

        org.bukkit.event.inventory.InventoryClickEvent click =
            new org.bukkit.event.inventory.InventoryClickEvent(player, null);
        expect(click.getInventory() == inventory,
            "an inventory click replaced the open custom inventory");
        expect(click.getInventory().getHolder(false) == holder,
            "an inventory click lost the custom holder identity");

        org.bukkit.event.inventory.InventoryDragEvent drag =
            new org.bukkit.event.inventory.InventoryDragEvent(
                player, java.util.Set.of(0), null,
                org.bukkit.event.inventory.InventoryDragEvent.DragType.EVEN);
        expect(drag.getInventory() == inventory,
            "an inventory drag replaced the open custom inventory");
        expect(drag.getInventory().getHolder(false) == holder,
            "an inventory drag lost the custom holder identity");

        org.bukkit.plugin.Plugin owner = PluginHost.all()[0];
        boolean[] closeObserved = {false};
        org.bukkit.Bukkit.getPluginManager().registerEvent(
            org.bukkit.event.inventory.InventoryCloseEvent.class,
            new org.bukkit.event.Listener() {},
            org.bukkit.event.EventPriority.NORMAL,
            (listener, event) -> {
                org.bukkit.event.inventory.InventoryCloseEvent close =
                    (org.bukkit.event.inventory.InventoryCloseEvent) event;
                if (!id.equals(close.getPlayer().getUniqueId().toString())) return;
                expect(close.getInventory() == inventory,
                    "an inventory close replaced the open custom inventory");
                expect(close.getInventory().getHolder(false) == holder,
                    "an inventory close lost the custom holder identity");
                closeObserved[0] = true;
                throw new IllegalStateException("expected close handler failure");
            },
            owner);

        EventBridge.fireInventoryClose(id);
        expect(closeObserved[0], "the custom inventory close handler did not run");
        expect(FotonCustomInventory.openForViewer(id) == null,
            "a failing close handler left the custom inventory attached");

        sameInstanceReopenSurvivesClose(owner);
        laterHandlerReopenSurvivesEarlierFailure(owner);
    }

    private static void sameInstanceReopenSurvivesClose(org.bukkit.plugin.Plugin owner) {
        String id = "00000000-0000-0000-0000-000000000020";
        FotonCustomInventory inventory = customInventory("same instance");
        inventory.attachViewer(id);
        org.bukkit.Bukkit.getPluginManager().registerEvent(
            org.bukkit.event.inventory.InventoryCloseEvent.class,
            new org.bukkit.event.Listener() {},
            org.bukkit.event.EventPriority.NORMAL,
            (listener, event) -> {
                org.bukkit.event.inventory.InventoryCloseEvent close =
                    (org.bukkit.event.inventory.InventoryCloseEvent) event;
                if (id.equals(close.getPlayer().getUniqueId().toString())) {
                    inventory.attachViewer(id);
                }
            },
            owner);

        EventBridge.fireInventoryClose(id);
        expect(FotonCustomInventory.openForViewer(id) == inventory,
            "closing detached the same custom inventory reopened by its handler");
        inventory.detachViewer();
    }

    private static void laterHandlerReopenSurvivesEarlierFailure(
            org.bukkit.plugin.Plugin owner) {
        String id = "00000000-0000-0000-0000-000000000021";
        FotonCustomInventory closing = customInventory("closing");
        FotonCustomInventory replacement = customInventory("replacement");
        closing.attachViewer(id);
        org.bukkit.Bukkit.getPluginManager().registerEvent(
            org.bukkit.event.inventory.InventoryCloseEvent.class,
            new org.bukkit.event.Listener() {},
            org.bukkit.event.EventPriority.LOW,
            (listener, event) -> {
                org.bukkit.event.inventory.InventoryCloseEvent close =
                    (org.bukkit.event.inventory.InventoryCloseEvent) event;
                if (id.equals(close.getPlayer().getUniqueId().toString())) {
                    throw new IllegalStateException("expected early close handler failure");
                }
            },
            owner);
        org.bukkit.Bukkit.getPluginManager().registerEvent(
            org.bukkit.event.inventory.InventoryCloseEvent.class,
            new org.bukkit.event.Listener() {},
            org.bukkit.event.EventPriority.NORMAL,
            (listener, event) -> {
                org.bukkit.event.inventory.InventoryCloseEvent close =
                    (org.bukkit.event.inventory.InventoryCloseEvent) event;
                if (id.equals(close.getPlayer().getUniqueId().toString())) {
                    replacement.attachViewer(id);
                }
            },
            owner);

        EventBridge.fireInventoryClose(id);
        expect(FotonCustomInventory.openForViewer(id) == replacement,
            "closing detached the replacement opened after a failing handler");
        replacement.detachViewer();
    }

    private static FotonCustomInventory customInventory(String title) {
        org.bukkit.inventory.Inventory[] held = {null};
        org.bukkit.inventory.InventoryHolder holder = () -> held[0];
        FotonCustomInventory inventory = new FotonCustomInventory(holder, 9, title);
        held[0] = inventory;
        return inventory;
    }

    private static void expect(boolean condition, String message) {
        if (!condition) throw new AssertionError(message);
    }
}
