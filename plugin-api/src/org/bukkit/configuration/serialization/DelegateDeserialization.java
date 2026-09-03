package org.bukkit.configuration.serialization;
import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;
/** Declares the class responsible for deserializing a configuration value. */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.TYPE)
public @interface DelegateDeserialization { Class<? extends ConfigurationSerializable> value(); }
