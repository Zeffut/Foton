package foton.item;

import org.bukkit.Material;
import org.bukkit.inventory.meta.*;

/** Internal Paper family identity; not a replacement for the public meta interfaces. */
public final class MetaFamily {
    private MetaFamily() {}

    public static SimpleItemMeta empty(Material material) {
        String family = foton.Native.itemMetaKind("minecraft:" + material.getKeyName());
        SimpleItemMeta meta = switch (family) {
            case "BOOK_WRITABLE", "BOOK_WRITTEN" -> new SimpleBookMeta();
            case "SKULL" -> new SimpleSkullMeta();
            case "COLORABLE_ARMOR", "LEATHER_ARMOR" -> new SimpleLeatherArmorMeta();
            case "POTION" -> new SimplePotionMeta();
            case "MAP" -> new SimpleMapMeta();
            case "FIREWORK" -> new SimpleFireworkMeta();
            case "FIREWORK_EFFECT" -> new SimpleFireworkEffectMeta();
            case "ENCHANTMENT_STORAGE" -> new SimpleEnchantmentStorageMeta();
            case "BANNER" -> new SimpleBannerMeta();
            case "BLOCK_STATE" -> new SimpleBlockStateMeta();
            case "CROSSBOW" -> new SimpleCrossbowMeta();
            case "SUSPICIOUS_STEW" -> new SimpleSuspiciousStewMeta();
            case "BUNDLE" -> new SimpleBundleMeta();
            default -> new SimpleItemMeta();
        };
        meta.setNativeFamily(family, material);
        return meta;
    }

    public static boolean applicable(String donor, String target) {
        if (target.equals("EMPTY")) return false;
        if (donor.equals("BASE")) return true;
        return donor.equals(target) || donor.equals("LEATHER_ARMOR") && target.equals("COLORABLE_ARMOR");
    }
}
