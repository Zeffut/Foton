package org.bukkit.event;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/** Marks a method as an event handler.
 *
 * Kept at runtime because that is the whole mechanism: a plugin does not
 * register its handlers, it annotates them and hands over the object.
 */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.METHOD)
public @interface EventHandler {
    EventPriority priority() default EventPriority.NORMAL;

    /** Whether this handler ignores an event that was already cancelled. */
    boolean ignoreCancelled() default false;
}
