import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;

/** Executed by the Rust slot tests with descriptions emitted by the real native bridge. */
public final class NativeItemBridgeCheck {
    public static void main(String[] args) throws Exception {
        BufferedReader input = new BufferedReader(
            new InputStreamReader(System.in, StandardCharsets.UTF_8));
        String first = input.readLine();
        if (args[0].equals("reject")) {
            try {
                foton.FotonInventory.decode(first);
                throw new AssertionError("An unencodable native item was accepted");
            } catch (IllegalStateException expected) {
                System.out.println(first);
            }
            return;
        }
        if (args[0].equals("book")) {
            org.bukkit.inventory.ItemStack item = foton.FotonInventory.decode(first);
            if (item == null || item.getType() != org.bukkit.Material.WRITTEN_BOOK)
                throw new AssertionError("Written book was lost in Java");
            org.bukkit.inventory.meta.BookMeta book = (org.bukkit.inventory.meta.BookMeta) item.getItemMeta();
            if (!"Livre de Cuisine".equals(book.getTitle())
                    || !"Les cuisiniers d'Hyrule".equals(book.getAuthor())
                    || book.getPageCount() != 1)
                throw new AssertionError("Written book cover or page was lost in Java");
            String pageJson = foton.ComponentJson.json(book.pages().get(0));
            if (!pageJson.contains("minecraft:cuisine") || !pageJson.contains("Radis"))
                throw new AssertionError("Styled glyph page was lost in Java: " + pageJson);
            var marker = new org.bukkit.NamespacedKey("zeldaciv", "livre_cuisine");
            if (!Integer.valueOf(3).equals(book.getPersistentDataContainer().get(marker,
                    org.bukkit.persistence.PersistentDataType.INTEGER)))
                throw new AssertionError("Written book PDC was lost in Java");
            System.out.println(foton.FotonInventory.encode(item));
            return;
        }
        org.bukkit.inventory.ItemStack one = foton.FotonInventory.decode(first);
        org.bukkit.inventory.ItemStack two = foton.FotonInventory.decode(input.readLine());
        if (one.isSimilar(two)) throw new AssertionError("Distinct opaque native data became similar");
        System.out.println(foton.FotonInventory.encode(one));
        System.out.println(foton.FotonInventory.encode(two));
    }
}
