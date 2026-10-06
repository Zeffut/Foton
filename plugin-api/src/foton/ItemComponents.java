package foton;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HexFormat;
import java.util.List;
import java.util.Locale;
import java.util.UUID;
import com.destroystokyo.paper.profile.PlayerProfile;
import com.destroystokyo.paper.profile.ProfileProperty;
import net.kyori.adventure.text.Component;
import org.bukkit.DyeColor;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;
import org.bukkit.Registry;
import org.bukkit.attribute.Attribute;
import org.bukkit.attribute.AttributeModifier;
import org.bukkit.block.banner.Pattern;
import org.bukkit.block.banner.PatternType;
import org.bukkit.inventory.EquipmentSlotGroup;
import org.bukkit.inventory.meta.BookMeta;
import org.bukkit.inventory.meta.SimpleBookMeta;
import org.bukkit.inventory.meta.SimpleItemMeta;
import org.bukkit.inventory.meta.SimpleSkullMeta;
import org.bukkit.inventory.meta.SkullMeta;
import org.bukkit.inventory.meta.trim.ArmorTrim;
import org.bukkit.inventory.meta.trim.TrimMaterial;
import org.bukkit.inventory.meta.trim.TrimPattern;

/** The vanilla components of an item that cross JNI beside the ones
 * {@link FotonInventory} has always carried.
 *
 * <p>One field per component, {@code name=value}, in the slot string's
 * {@code \u001d}-separated list; foton-plugin's {@code describe_slot} and
 * {@code parse_slot} write and read the same fields, so a plugin's banner,
 * trim, cooldown or custom data is the item the client sees and the world
 * saves.</p>
 */
final class ItemComponents {
    private ItemComponents() { }

    private static final String SEP = "\u001d";

    static String encode(SimpleItemMeta meta, Material type) {
        StringBuilder out = new StringBuilder();
        if (meta.displayName() != null) field(out, "namejsonhex", hex(ComponentJson.json(meta.displayName())));
        if (meta.hasLore()) for (Component line : meta.lore()) field(out, "lorejsonhex", hex(ComponentJson.json(line)));
        for (String hidden : meta.getHiddenComponents()) field(out, "hide", hidden);
        if (meta.hasEnchantmentGlintOverride()) field(out, "glint", String.valueOf(meta.getEnchantmentGlintOverride()));
        if (meta.hasMaxStackSize()) field(out, "maxstack", String.valueOf(meta.getMaxStackSize()));
        if (meta.hasUseCooldown()) {
            field(out, "cooldown", String.valueOf(meta.getUseCooldown().getCooldownSeconds()));
            NamespacedKey group = meta.getUseCooldown().getCooldownGroup();
            if (group != null) field(out, "cooldowngroup", group.toString());
        }
        List<Pattern> patterns = meta.getBannerPatternsComponent();
        if (patterns != null) {
            for (Pattern pattern : patterns) {
                if (pattern.getPattern() == null || pattern.getColor() == null) continue;
                field(out, "pattern", pattern.getPattern().getKey() + "," + dye(pattern.getColor()));
            }
        }
        if (meta.getBaseColorComponent() != null) field(out, "basecolor", dye(meta.getBaseColorComponent()));
        ArmorTrim trim = meta.getTrimComponent();
        if (trim != null) field(out, "trim", trim.getMaterial().getKey() + "," + trim.getPattern().getKey());
        if (meta.getInstrumentComponent() != null) field(out, "instrument", meta.getInstrumentComponent().getKey().toString());
        for (java.util.Map.Entry<Attribute, AttributeModifier> entry : meta.getAttributeModifiers().entries()) {
            AttributeModifier modifier = entry.getValue();
            field(out, "attrmod", entry.getKey().getKey() + "," + modifier.getKey() + "," + modifier.getAmount() + ","
                + operation(modifier.getOperation()) + "," + modifier.getSlotGroup().name().toLowerCase(Locale.ROOT));
        }
        if (meta instanceof SkullMeta skull && skull.getPlayerProfile() instanceof PlayerProfile profile) {
            if (profile.getUniqueId() != null) field(out, "profileid", profile.getUniqueId().toString());
            if (profile.getName() != null) field(out, "profilename", hex(profile.getName()));
            for (ProfileProperty property : profile.getProperties())
                field(out, "profileprop", hex(property.getName()) + "," + hex(property.getValue()) + ","
                    + (property.getSignature() == null ? "" : hex(property.getSignature())));
        }
        java.util.Map<String, Object> custom = meta.getCustomData();
        if (!custom.isEmpty()) field(out, "customhex", hex(Snbt.write(custom)));
        return out.toString();
    }

    static void decode(SimpleItemMeta meta, Material type, List<String> fields) {
        List<String> hidden = new ArrayList<>();
        List<Pattern> patterns = new ArrayList<>();
        List<Component> lore = new ArrayList<>();
        Float cooldown = null;
        NamespacedKey cooldownGroup = null;
        UUID profileId = null;
        String profileName = null;
        List<ProfileProperty> profileProperties = new ArrayList<>();
        for (String field : fields) {
            int equals = field.indexOf('=');
            if (equals < 0) continue;
            String name = field.substring(0, equals);
            String value = field.substring(equals + 1);
            try {
                switch (name) {
                    case "namejsonhex" -> meta.displayName(ComponentJson.parse(unhex(value)));
                    case "lorejsonhex" -> lore.add(ComponentJson.parse(unhex(value)));
                    case "hide" -> hidden.add(value);
                    case "glint" -> meta.setEnchantmentGlintOverride(Boolean.parseBoolean(value));
                    case "maxstack" -> meta.setMaxStackSize(Integer.parseInt(value));
                    case "cooldown" -> cooldown = Float.parseFloat(value);
                    case "cooldowngroup" -> cooldownGroup = NamespacedKey.fromString(value);
                    case "pattern" -> {
                        String[] parts = value.split(",", 2);
                        PatternType pattern = Registry.BANNER_PATTERN.get(NamespacedKey.fromString(parts[0]));
                        DyeColor color = dye(parts[1]);
                        if (pattern != null && color != null) patterns.add(new Pattern(color, pattern));
                    }
                    case "basecolor" -> meta.setBaseColorComponent(dye(value));
                    case "trim" -> {
                        String[] parts = value.split(",", 2);
                        TrimMaterial material = Registry.TRIM_MATERIAL.get(NamespacedKey.fromString(parts[0]));
                        TrimPattern pattern = Registry.TRIM_PATTERN.get(NamespacedKey.fromString(parts[1]));
                        if (material != null && pattern != null) meta.setTrimComponent(new ArmorTrim(material, pattern));
                    }
                    case "instrument" -> meta.setInstrumentComponent(Registry.INSTRUMENT.get(NamespacedKey.fromString(value)));
                    case "customhex" -> meta.setCustomData(Snbt.parseCompound(unhex(value)));
                    case "attrmod" -> {
                        String[] parts = value.split(",");
                        Attribute attribute = Registry.ATTRIBUTE.get(NamespacedKey.fromString(parts[0]));
                        EquipmentSlotGroup slot = EquipmentSlotGroup.getByName(parts[4]);
                        AttributeModifier.Operation operation = operation(parts[3]);
                        if (attribute != null && slot != null && operation != null)
                            meta.hydrateAttributeModifier(attribute, new AttributeModifier(
                                NamespacedKey.fromString(parts[1]), Double.parseDouble(parts[2]), operation, slot));
                    }
                    case "profileid" -> profileId = UUID.fromString(value);
                    case "profilename" -> profileName = unhex(value);
                    case "profileprop" -> {
                        String[] parts = value.split(",", -1);
                        String signature = unhex(parts[2]);
                        profileProperties.add(new ProfileProperty(unhex(parts[0]), unhex(parts[1]),
                            signature.isEmpty() ? null : signature));
                    }
                    default -> { }
                }
            } catch (RuntimeException unreadable) {
                // One malformed field loses that component, not the item.
            }
        }
        if (!lore.isEmpty()) meta.lore(lore);
        if (!hidden.isEmpty()) meta.setHiddenComponents(hidden);
        if (!patterns.isEmpty()) meta.setBannerPatternsComponent(patterns);
        if (cooldown != null && cooldown > 0) {
            meta.setUseCooldown(new org.bukkit.inventory.meta.components.SimpleUseCooldownComponent(cooldown, cooldownGroup));
        }
        if (meta instanceof SimpleSkullMeta skull && (profileId != null || profileName != null || !profileProperties.isEmpty())) {
            FotonPlayerProfile profile = new FotonPlayerProfile(profileId, profileName);
            profile.setProperties(profileProperties);
            skull.hydrateOwnerProfile(profile);
        }
    }

    /** Paper's mapping onto vanilla's operation names. */
    private static String operation(AttributeModifier.Operation operation) {
        return switch (operation) {
            case ADD_NUMBER -> "add_value";
            case ADD_SCALAR -> "add_multiplied_base";
            case MULTIPLY_SCALAR_1 -> "add_multiplied_total";
        };
    }

    private static AttributeModifier.Operation operation(String name) {
        return switch (name) {
            case "add_value" -> AttributeModifier.Operation.ADD_NUMBER;
            case "add_multiplied_base" -> AttributeModifier.Operation.ADD_SCALAR;
            case "add_multiplied_total" -> AttributeModifier.Operation.MULTIPLY_SCALAR_1;
            default -> null;
        };
    }

    private static void field(StringBuilder out, String name, String value) {
        out.append(SEP).append(name).append('=').append(value);
    }

    private static String dye(DyeColor color) {
        return color.name().toLowerCase(Locale.ROOT);
    }

    private static DyeColor dye(String name) {
        try {
            return DyeColor.valueOf(name.toUpperCase(Locale.ROOT));
        } catch (IllegalArgumentException unknown) {
            return null;
        }
    }

    static String hex(String value) {
        return HexFormat.of().formatHex(value.getBytes(StandardCharsets.UTF_8));
    }

    static String unhex(String value) {
        return new String(HexFormat.of().parseHex(value), StandardCharsets.UTF_8);
    }
}
