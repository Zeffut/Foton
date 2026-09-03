package org.bukkit.event.inventory;

import org.bukkit.entity.HumanEntity;
import org.bukkit.inventory.Recipe;

/** Fired when a crafting recipe produces an item. */
public class CraftItemEvent extends InventoryClickEvent {
    private final Recipe recipe;
    public CraftItemEvent(HumanEntity whoClicked, Recipe recipe) { super(whoClicked); this.recipe = recipe; }
    public Recipe getRecipe() { return recipe; }
}
