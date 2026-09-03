package org.bukkit.plugin;

import org.bukkit.event.Event;
import org.bukkit.event.EventException;
import org.bukkit.event.Listener;

/** The callable behind a handler registered without an annotation. */
public interface EventExecutor {
    void execute(Listener listener, Event event) throws EventException;

    /** Creates a reflective executor for Bukkit's manually registered handlers. */
    static EventExecutor create(java.lang.reflect.Method method, Class<? extends Event> eventClass) {
        if (method == null || eventClass == null) throw new IllegalArgumentException("method and eventClass");
        if (method.getParameterCount() != 1 || !method.getParameterTypes()[0].isAssignableFrom(eventClass))
            throw new IllegalArgumentException("handler must accept the event type");
        return (listener, event) -> {
            try {
                method.invoke(listener, event);
            } catch (java.lang.reflect.InvocationTargetException exception) {
                throw new EventException(exception.getCause());
            } catch (ReflectiveOperationException | IllegalArgumentException exception) {
                throw new EventException(exception);
            }
        };
    }
}
