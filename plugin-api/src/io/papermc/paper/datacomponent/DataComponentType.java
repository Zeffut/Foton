package io.papermc.paper.datacomponent;

/** Identifies a vanilla data component. */
public interface DataComponentType<T> {
    interface Valued<T> extends DataComponentType<T> { }
}
