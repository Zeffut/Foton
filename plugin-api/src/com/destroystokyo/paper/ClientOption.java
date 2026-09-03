package com.destroystokyo.paper;

public final class ClientOption<T> {
    public static final ClientOption<String> LOCALE = new ClientOption<>("locale");
    private final String name;
    private ClientOption(String name) { this.name = name; }
    public String getKey() { return name; }
}
