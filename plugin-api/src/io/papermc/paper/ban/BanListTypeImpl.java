package io.papermc.paper.ban;

record BanListTypeImpl<T>(Class<T> typeClass) implements BanListType<T> {
}
