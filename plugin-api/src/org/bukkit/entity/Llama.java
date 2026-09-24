package org.bukkit.entity;

/** Vanilla llama entity view. */
public interface Llama extends ChestedHorse {
    enum Color { CREAMY, WHITE, BROWN, GRAY }
    @Override org.bukkit.inventory.LlamaInventory getInventory();
    Color getColor();
    void setColor(Color color);
}
