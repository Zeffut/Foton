package org.bukkit.configuration.file;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.Reader;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import org.bukkit.configuration.InvalidConfigurationException;
import org.bukkit.configuration.MemoryConfiguration;

/** A configuration that lives in a file. */
public abstract class FileConfiguration extends MemoryConfiguration {
    public abstract String saveToString();

    public abstract void loadFromString(String contents) throws InvalidConfigurationException;

    public void save(File file) throws IOException {
        Path target = file.toPath();
        Path parent = target.getParent();
        if (parent != null) {
            Files.createDirectories(parent);
        }
        Files.write(target, saveToString().getBytes(StandardCharsets.UTF_8));
    }

    public void save(String file) throws IOException {
        save(new File(file));
    }

    public void load(File file) throws IOException, InvalidConfigurationException {
        loadFromString(new String(Files.readAllBytes(file.toPath()), StandardCharsets.UTF_8));
    }

    public void load(Reader reader) throws IOException, InvalidConfigurationException {
        StringBuilder text = new StringBuilder();
        char[] buffer = new char[4096];
        int read;
        while ((read = reader.read(buffer)) != -1) {
            text.append(buffer, 0, read);
        }
        loadFromString(text.toString());
    }

    public void load(InputStream stream) throws IOException, InvalidConfigurationException {
        load(new InputStreamReader(stream, StandardCharsets.UTF_8));
    }
}
