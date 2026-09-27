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
        org.bukkit.inventory.ItemStack one = foton.FotonInventory.decode(first);
        org.bukkit.inventory.ItemStack two = foton.FotonInventory.decode(input.readLine());
        if (one.isSimilar(two)) throw new AssertionError("Distinct opaque native data became similar");
        System.out.println(foton.FotonInventory.encode(one));
        System.out.println(foton.FotonInventory.encode(two));
    }
}
