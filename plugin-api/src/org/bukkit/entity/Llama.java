package org.bukkit.entity;

/** Vanilla llama entity view. */
public interface Llama extends ChestedHorse {
    enum Color { CREAMY, WHITE, BROWN, GRAY }
    default org.bukkit.inventory.LlamaInventory getInventory() { return null; }
    Color getColor();
    void setColor(Color color);
}
