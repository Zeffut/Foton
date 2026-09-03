package org.bukkit.entity;

/** Vanilla panda entity view. */
public interface Panda extends Animal {
    enum Gene { NORMAL, LAZY, WORRIED, PLAYFUL, BROWN, WEAK, AGGRESSIVE }
    Gene getMainGene();
    void setMainGene(Gene gene);
    Gene getHiddenGene();
    void setHiddenGene(Gene gene);
}
