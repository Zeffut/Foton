package foton;

import net.kyori.adventure.text.Component;
import net.md_5.bungee.api.chat.BaseComponent;

/** BungeeCord chat components, as the Adventure components Foton sends.
 *
 * The two describe the same JSON text, so the conversion goes through it and
 * keeps everything: translations, colours, hovers, clicks. Flattening to
 * legacy text, as a plugin's Spigot call used to be, kept only the colours.
 */
public final class FotonBungee {
    private FotonBungee() {}

    /** The components as one, or an empty component for none. */
    public static Component component(BaseComponent... components) {
        if (components == null || components.length == 0) return Component.empty();
        java.util.List<BaseComponent> present = new java.util.ArrayList<>();
        for (BaseComponent component : components) if (component != null) present.add(component);
        if (present.isEmpty()) return Component.empty();
        String json = net.md_5.bungee.chat.ComponentSerializer.toString(present.toArray(new BaseComponent[0]));
        return FotonText.component(json);
    }
}
