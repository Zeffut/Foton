package org.bukkit;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/** Controls whether a deprecation or compatibility warning is printed. */
@Retention(RetentionPolicy.RUNTIME)
@Target({ElementType.TYPE, ElementType.METHOD, ElementType.CONSTRUCTOR, ElementType.FIELD})
public @interface Warning {
    String reason() default "";
    WarningState state() default WarningState.DEFAULT;

    enum WarningState {
        DEFAULT, ALWAYS, NEVER;

        public boolean printFor(Warning warning) {
            if (this == ALWAYS) return true;
            if (this == NEVER) return false;
            return warning == null || warning.state() != NEVER;
        }
    }
}
