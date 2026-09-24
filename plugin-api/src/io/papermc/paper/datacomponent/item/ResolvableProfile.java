package io.papermc.paper.datacomponent.item;

import com.destroystokyo.paper.profile.PlayerProfile;
import com.destroystokyo.paper.profile.ProfileProperty;
import io.papermc.paper.datacomponent.DataComponentBuilder;
import java.util.Collection;
import java.util.UUID;
import net.kyori.adventure.key.Key;
import net.kyori.adventure.text.object.PlayerHeadObjectContents;

/** A game profile as an item or a mannequin carries it: a name, an id, and skin properties.
 *
 * <p>An interface, with a builder interface, as in Paper: plugins call
 * {@code ResolvableProfile.resolvableProfile()} and the builder's methods with
 * {@code invokeinterface}, which a class answers with
 * {@code IncompatibleClassChangeError}.</p>
 *
 * <p>Paper's {@code dynamic()} and {@code resolve()} are absent: resolving a
 * partial profile means asking Mojang's session servers, which Foton does not
 * do for plugins. A plugin that calls them fails with {@code NoSuchMethodError}
 * rather than receiving a profile nobody looked up.</p>
 */
public interface ResolvableProfile extends PlayerHeadObjectContents.SkinSource {
    static ResolvableProfile resolvableProfile(PlayerProfile profile) {
        Builder builder = resolvableProfile().uuid(profile.getId()).name(profile.getName());
        return builder.addProperties(profile.getProperties()).build();
    }

    static Builder resolvableProfile() {
        return new foton.FotonResolvableProfile.Builder();
    }

    UUID uuid();

    String name();

    Collection<ProfileProperty> properties();

    SkinPatch skinPatch();

    /** Textures that override the profile's own, by asset key. */
    interface SkinPatch {
        static SkinPatch empty() {
            return foton.FotonResolvableProfile.EMPTY_PATCH;
        }

        static SkinPatch skinPatch(Key body, Key cape, Key elytra) {
            return new foton.FotonResolvableProfile.Patch(body, cape, elytra);
        }

        Key body();

        Key cape();

        Key elytra();

        default boolean isEmpty() {
            return body() == null && cape() == null && elytra() == null;
        }
    }

    interface Builder extends DataComponentBuilder<ResolvableProfile> {
        Builder name(String name);

        Builder uuid(UUID uuid);

        Builder addProperty(ProfileProperty property);

        Builder addProperties(Collection<ProfileProperty> properties);

        Builder skinPatch(SkinPatch patch);
    }
}
