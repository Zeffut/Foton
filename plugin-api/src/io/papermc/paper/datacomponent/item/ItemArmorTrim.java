package io.papermc.paper.datacomponent.item;

import io.papermc.paper.datacomponent.DataComponentBuilder;
import java.util.Objects;
import org.bukkit.inventory.meta.trim.ArmorTrim;

/** The {@code trim} component: an armor piece's trim pattern and material. */
public interface ItemArmorTrim {
    static Builder itemArmorTrim(ArmorTrim armorTrim) {
        return new Builder() {
            private ArmorTrim trim = Objects.requireNonNull(armorTrim, "armorTrim");

            @Override
            public Builder armorTrim(ArmorTrim value) {
                trim = Objects.requireNonNull(value, "armorTrim");
                return this;
            }

            @Override
            public ItemArmorTrim build() {
                return new Trim(trim);
            }
        };
    }

    ArmorTrim armorTrim();

    interface Builder extends DataComponentBuilder<ItemArmorTrim> {
        Builder armorTrim(ArmorTrim armorTrim);
    }

    /** The one implementation. */
    record Trim(ArmorTrim armorTrim) implements ItemArmorTrim { }
}
